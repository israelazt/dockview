//! Going live on this PC: a thread that takes the pictures and the sound,
//! encodes them (GPU H.264 and AAC) and hands them to the portable
//! `StreamOutput`, which talks RTMP. The rules (reconnecting, mixing, the
//! protocol) live in `dockview-stream`; this is the glue to Windows.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dockview_domain::broadcast::{
    AudioEncoder, StreamHandle, StreamMachine, StreamRequest, StreamState, StreamStats,
    VideoEncoder, VideoEncoderConfig,
};
use dockview_domain::frame::{FrameSlot, FrameTap};
use dockview_domain::pcm::AudioTap;
use dockview_stream::audio_pump::AudioPump;
use dockview_stream::output::{StreamConfig, StreamOutput};
use dockview_stream::tcp::TcpConnector;
use dockview_stream::transport::Target;

use crate::encode::{AacEncoder, LiveVideoEncoder};
use crate::mf::MfSession;
use crate::worker::Worker;

/// Pictures the tap may hold (a little over 130 ms at 60 fps).
const TAP_CAPACITY: usize = 8;
/// Audio blocks each tap may hold (about 640 ms of 10 ms blocks).
const AUDIO_TAP_CAPACITY: usize = 64;
/// How long the thread waits for a picture before doing its other chores.
const POLL: Duration = Duration::from_millis(15);
/// How long `Drop` waits for the thread (a send may be stuck on the network).
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
/// The longest a failed stream is retried: connection attempts in a row.
const MAX_ATTEMPTS: u32 = 12;
/// Asking the encoder for a keyframe more often than this is pointless.
const KEYFRAME_REQUEST_EVERY: Duration = Duration::from_secs(1);

/// The machine the app really runs on.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlatformStream;

impl StreamMachine for PlatformStream {
    fn start_stream(
        &self,
        slot: Arc<FrameSlot>,
        request: StreamRequest,
    ) -> Result<Box<dyn StreamHandle>, String> {
        let target = Target::parse(&request.url)
            .ok_or("the server address must look like rtmp://host/app or rtmps://host/app")?;
        if request.key.trim().is_empty() {
            return Err("the stream key is empty".into());
        }
        if !(1..=240).contains(&request.fps) || request.video_kbps == 0 {
            return Err("the frame rate or the bitrate is not valid".into());
        }
        Ok(Box::new(LiveStream::start(slot, target, request)?))
    }
}

#[derive(Default)]
struct Shared {
    stats: Mutex<StreamStats>,
    error: Mutex<Option<String>>,
}

impl Shared {
    fn publish(&self, stats: StreamStats, error: Option<&str>) {
        *self.stats.lock().unwrap_or_else(|e| e.into_inner()) = stats;
        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = error.map(str::to_string);
    }

    fn fail(&self, message: String) {
        self.publish(
            StreamStats {
                state: StreamState::Failed,
                ..StreamStats::default()
            },
            Some(&message),
        );
    }
}

/// A stream while it runs. Dropping it ends the stream.
pub struct LiveStream {
    slot: Arc<FrameSlot>,
    audio_tap: Arc<AudioTap>,
    mic_tap: Arc<AudioTap>,
    shared: Arc<Shared>,
    /// Dropped after `Drop::drop` detached the tap: stops the thread.
    _worker: Worker,
}

impl LiveStream {
    fn start(slot: Arc<FrameSlot>, target: Target, request: StreamRequest) -> Result<Self, String> {
        let tap = Arc::new(FrameTap::new(TAP_CAPACITY));
        let audio_tap = Arc::new(AudioTap::new(AUDIO_TAP_CAPACITY));
        let mic_tap = Arc::new(AudioTap::new(AUDIO_TAP_CAPACITY));
        let shared = Arc::new(Shared::default());
        shared.publish(
            StreamStats {
                state: StreamState::Connecting,
                ..StreamStats::default()
            },
            None,
        );
        let mut worker = Worker::new(STOP_TIMEOUT);
        let inputs = Inputs {
            tap: Arc::clone(&tap),
            audio: Arc::clone(&audio_tap),
            mic: Arc::clone(&mic_tap),
        };
        let thread_shared = Arc::clone(&shared);
        let started = worker.spawn("live-stream", move |stop| {
            run(&inputs, &thread_shared, stop, target, request);
        });
        if !started {
            return Err("could not start the stream thread".into());
        }
        slot.set_stream_tap(Some(tap));
        Ok(Self {
            slot,
            audio_tap,
            mic_tap,
            shared,
            _worker: worker,
        })
    }
}

impl Drop for LiveStream {
    fn drop(&mut self) {
        self.slot.set_stream_tap(None);
    }
}

impl StreamHandle for LiveStream {
    fn stats(&self) -> StreamStats {
        *self.shared.stats.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn last_error(&self) -> Option<String> {
        self.shared
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn audio_tap(&self) -> Arc<AudioTap> {
        Arc::clone(&self.audio_tap)
    }

    fn mic_tap(&self) -> Arc<AudioTap> {
        Arc::clone(&self.mic_tap)
    }
}

struct Inputs {
    tap: Arc<FrameTap>,
    audio: Arc<AudioTap>,
    mic: Arc<AudioTap>,
}

/// Everything that exists once the first picture has told the size.
struct Live {
    video: LiveVideoEncoder,
    output: StreamOutput,
    width: u32,
    height: u32,
    last_encoded: i64,
    last_keyframe_request: Option<Instant>,
}

fn ticks_since(epoch: Instant, at: Instant) -> i64 {
    (at.saturating_duration_since(epoch).as_nanos() / 100) as i64
}

fn run(
    inputs: &Inputs,
    shared: &Shared,
    stop: &AtomicBool,
    target: Target,
    request: StreamRequest,
) {
    use std::sync::atomic::Ordering;

    let session = match MfSession::start() {
        Ok(s) => s,
        Err(e) => return shared.fail(e.message()),
    };
    let mut aac = match AacEncoder::new(&session) {
        Ok(a) => a,
        Err(e) => return shared.fail(format!("no AAC encoder: {}", e.message())),
    };
    let epoch = Instant::now();
    let mut pump = AudioPump::new(0);
    let mut live: Option<Live> = None;
    let interval = 10_000_000 / i64::from(request.fps);

    while !stop.load(Ordering::Relaxed) {
        let frame = inputs.tap.take(POLL);
        let now = Instant::now();
        if let Some(mut frame) = frame {
            frame.timestamp = ticks_since(epoch, frame.arrived);
            if live.is_none() {
                match open(&session, &aac, &frame, &target, &request, now) {
                    Ok(l) => live = Some(l),
                    Err(e) => return shared.fail(e),
                }
            }
            if let Some(l) = live.as_mut() {
                if let Err(e) = send_picture(l, &frame, interval, now) {
                    return shared.fail(e);
                }
            }
            inputs.tap.recycle(frame);
        }
        while let Some(c) = inputs.audio.take(Duration::ZERO) {
            pump.push_game(ticks_since(epoch, c.start), &c.samples);
        }
        while let Some(c) = inputs.mic.take(Duration::ZERO) {
            pump.push_mic(ticks_since(epoch, c.start), &c.samples);
        }
        if let Some(l) = live.as_mut() {
            match pump.pump(ticks_since(epoch, now), &mut aac) {
                Ok(units) => units.iter().for_each(|u| l.output.push_audio(u, now)),
                Err(e) => return shared.fail(format!("audio: {e}")),
            }
            l.output.tick(now);
            let stats = l.output.stats();
            shared.publish(stats, l.output.last_error());
            if stats.state == StreamState::Failed {
                return;
            }
        }
    }
    if let Some(mut l) = live {
        l.output.stop();
        shared.publish(l.output.stats(), None);
    }
}

fn open(
    session: &MfSession,
    aac: &AacEncoder,
    first: &dockview_domain::frame::Frame,
    target: &Target,
    request: &StreamRequest,
    now: Instant,
) -> Result<Live, String> {
    let config =
        VideoEncoderConfig::live(first.width, first.height, request.fps, request.video_kbps);
    let video = LiveVideoEncoder::new(session, config).map_err(|e| e.message())?;
    let mut output = StreamOutput::new(
        Box::new(TcpConnector::default()),
        target.clone(),
        &request.key,
        StreamConfig {
            width: first.width,
            height: first.height,
            fps: request.fps,
            video_kbps: request.video_kbps,
            audio: Some(aac.config()),
            max_attempts: MAX_ATTEMPTS,
        },
    );
    output.start(now);
    Ok(Live {
        video,
        output,
        width: first.width,
        height: first.height,
        last_encoded: i64::MIN / 2,
        last_keyframe_request: None,
    })
}

fn send_picture(
    l: &mut Live,
    frame: &dockview_domain::frame::Frame,
    interval: i64,
    now: Instant,
) -> Result<(), String> {
    if (frame.width, frame.height) != (l.width, l.height) {
        return Err(format!(
            "the picture changed size ({}x{} to {}x{}); stop and start the stream again",
            l.width, l.height, frame.width, frame.height
        ));
    }
    // The capture can run faster than the stream: keep to the stream's rate.
    if frame.timestamp - l.last_encoded < interval * 9 / 10 {
        return Ok(());
    }
    l.last_encoded = frame.timestamp;
    if l.output.wants_keyframe()
        && l.last_keyframe_request
            .is_none_or(|t| now.duration_since(t) >= KEYFRAME_REQUEST_EVERY)
    {
        l.video.request_keyframe();
        l.last_keyframe_request = Some(now);
    }
    for coded in l.video.encode(frame)? {
        l.output.push_video(&coded, now);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::hardware_or_skip;
    use dockview_domain::audio_sync::CHANNELS;
    use dockview_domain::frame::{ColorInfo, Frame, PixelFormat};
    use dockview_stream::testing::{serve_tcp, FakeServer};

    fn picture(n: u32) -> Frame {
        let (w, h) = (640u32, 360u32);
        let mut data = vec![128u8; (w * h * 3 / 2) as usize];
        for (i, b) in data.iter_mut().take((w * h) as usize).enumerate() {
            *b = ((i as u32 + n * 11) % 220) as u8 + 16;
        }
        Frame {
            width: w,
            height: h,
            stride: w,
            format: PixelFormat::Nv12,
            color: ColorInfo::default(),
            data,
            timestamp: 0,
            arrived: Instant::now(),
        }
    }

    fn request(port: u16) -> StreamRequest {
        StreamRequest {
            url: format!("rtmp://127.0.0.1:{port}/live"),
            key: "abc".into(),
            fps: 30,
            video_kbps: 2000,
        }
    }

    #[test]
    fn a_bad_address_or_key_is_refused_before_anything_starts() {
        let slot = Arc::new(FrameSlot::new());
        let machine = PlatformStream;
        let mut r = request(1);
        r.url = "http://example.com".into();
        assert!(machine.start_stream(slot.clone(), r).is_err());
        let mut r = request(1);
        r.key = "  ".into();
        assert!(machine.start_stream(slot.clone(), r).is_err());
        let mut r = request(1);
        r.fps = 0;
        assert!(machine.start_stream(slot, r).is_err());
    }

    #[test]
    fn pictures_and_sound_reach_a_server_with_the_headers_first() {
        if !hardware_or_skip("a hardware H.264 encoder and the AAC encoder") {
            return;
        }
        let server = FakeServer::default();
        let port = serve_tcp(server.clone());
        let slot = Arc::new(FrameSlot::new());
        let stream = PlatformStream
            .start_stream(slot.clone(), request(port))
            .unwrap();
        let game = stream.audio_tap();

        let begun = Instant::now();
        let mut n = 0;
        while begun.elapsed() < Duration::from_millis(3500) {
            slot.publish(picture(n));
            game.offer(Instant::now(), &vec![0.1; 480 * CHANNELS]);
            n += 1;
            std::thread::sleep(Duration::from_millis(33));
        }
        let stats = stream.stats();
        let error = stream.last_error();
        drop(stream);

        assert_eq!(stats.state, StreamState::Live, "{error:?}");
        assert!(stats.sent_frames > 40, "{stats:?}");
        let video = server.seen(|s| s.video.clone());
        let audio = server.seen(|s| s.audio.clone());
        assert_eq!(
            video[0].payload[..2],
            [0x17, 0],
            "the video header goes first"
        );
        assert_eq!(
            audio[0].payload[..2],
            [0xAF, 0],
            "the audio header goes first"
        );
        assert!(
            video.len() > 40 && audio.len() > 40,
            "{} / {}",
            video.len(),
            audio.len()
        );
        assert_eq!(server.seen(|s| s.keys.clone()), ["abc"]);
        assert!(video.windows(2).all(|w| w[1].timestamp >= w[0].timestamp));
    }

    #[test]
    fn dropping_the_stream_detaches_it_from_the_frames() {
        let slot = Arc::new(FrameSlot::new());
        let port = serve_tcp(FakeServer::default());
        let stream = PlatformStream
            .start_stream(slot.clone(), request(port))
            .unwrap();
        drop(stream);
        // A published picture no longer goes anywhere; nothing panics or blocks.
        slot.publish(picture(0));
    }
}
