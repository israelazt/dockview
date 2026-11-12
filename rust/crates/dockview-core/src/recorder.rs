//! Live replay recording.
//!
//! A background thread taps every video frame, compresses it with the GPU's
//! H.264 encoder and keeps the newest footage in a [`ReplayRing`]. Saving cuts
//! a window out of the ring and writes it to an MP4 *without re-encoding*, on
//! its own short-lived thread so the live view is never held up.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::encode::{sequence_header, EncodeSettings, EncodedFrame, H264Encoder, Mp4Remuxer};
use crate::frame::{Frame, FrameSlot, FrameTap, PixelFormat};
use crate::mf::MfSession;
use crate::replay::ReplayRing;

/// Frames the tap may hold before it starts dropping (about 130 ms at 60 fps).
const TAP_CAPACITY: usize = 8;
/// How long the recorder waits for a frame before checking for a stop request.
const POLL: Duration = Duration::from_millis(100);
/// How long `Drop` waits for the recorder thread.
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

/// What to record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplaySettings {
    /// Footage to keep, in seconds.
    pub keep_seconds: f64,
    /// Video bitrate in megabits per second.
    pub bitrate_mbps: u32,
    /// Nominal frame rate (sets the keyframe interval and frame duration).
    pub fps: u32,
}

impl Default for ReplaySettings {
    fn default() -> Self {
        Self {
            keep_seconds: 120.0,
            bitrate_mbps: 40,
            fps: 60,
        }
    }
}

/// State of the recorder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderStatus {
    /// Waiting for the first frame.
    Starting,
    Recording,
    /// This PC cannot record (no hardware encoder, or an unsupported format).
    Unsupported(String),
    Failed(String),
}

#[derive(Default)]
struct Shared {
    ring: Mutex<Option<ReplayRing>>,
    status: Mutex<Option<RecorderStatus>>,
    /// SPS/PPS of the stream, needed to build the MP4.
    header: Mutex<Option<Vec<u8>>>,
    /// Size and nominal settings of the stream being recorded.
    stream: Mutex<Option<EncodeSettings>>,
    encoded_frames: AtomicU64,
}

impl Shared {
    fn set_status(&self, status: RecorderStatus) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = Some(status);
    }
}

/// A finished clip.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedClip {
    pub path: PathBuf,
    pub frames: usize,
    pub seconds: f64,
    pub bytes: u64,
}

/// Result of a save running in the background.
pub struct SaveHandle {
    result: Arc<Mutex<Option<Result<SavedClip, String>>>>,
}

impl SaveHandle {
    /// The outcome once the save has finished.
    pub fn poll(&self) -> Option<Result<SavedClip, String>> {
        self.result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Blocks until the save finishes (for tests and tools).
    pub fn wait(&self, timeout: Duration) -> Option<Result<SavedClip, String>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(result) = self.poll() {
                return Some(result);
            }
            if Instant::now() > deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Records the newest footage while it is alive. Dropping it stops recording.
pub struct ReplayRecorder {
    slot: Arc<FrameSlot>,
    tap: Arc<FrameTap>,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ReplayRecorder {
    /// Starts recording every frame published to `slot`.
    pub fn start(slot: Arc<FrameSlot>, settings: ReplaySettings) -> Self {
        let tap = Arc::new(FrameTap::new(TAP_CAPACITY));
        let shared = Arc::new(Shared::default());
        *shared.ring.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(ReplayRing::new(settings.keep_seconds));
        shared.set_status(RecorderStatus::Starting);
        let stop = Arc::new(AtomicBool::new(false));

        let thread = {
            let (tap, shared, stop) = (Arc::clone(&tap), Arc::clone(&shared), Arc::clone(&stop));
            thread::Builder::new()
                .name("replay-recorder".into())
                .spawn(move || run(&tap, &shared, &stop, settings))
                .ok()
        };
        if thread.is_none() {
            shared.set_status(RecorderStatus::Failed(
                "could not start the recorder".into(),
            ));
        } else {
            slot.set_tap(Some(Arc::clone(&tap)));
        }
        Self {
            slot,
            tap,
            shared,
            stop,
            thread,
        }
    }

    pub fn status(&self) -> RecorderStatus {
        self.shared
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or(RecorderStatus::Starting)
    }

    /// Seconds of footage held right now.
    pub fn buffered_seconds(&self) -> f64 {
        self.with_ring(|r| r.duration_seconds()).unwrap_or(0.0)
    }

    /// Compressed bytes held right now.
    pub fn buffered_bytes(&self) -> usize {
        self.with_ring(|r| r.bytes()).unwrap_or(0)
    }

    /// Frames compressed since the start.
    pub fn encoded_frames(&self) -> u64 {
        self.shared.encoded_frames.load(Ordering::Relaxed)
    }

    /// Frames the recorder could not keep up with.
    pub fn dropped_frames(&self) -> u64 {
        self.tap.dropped()
    }

    /// Changes how much footage is kept.
    pub fn set_keep_seconds(&self, seconds: f64) {
        if let Some(ring) = self
            .shared
            .ring
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            ring.set_keep_seconds(seconds);
        }
    }

    fn with_ring<T>(&self, f: impl FnOnce(&ReplayRing) -> T) -> Option<T> {
        self.shared
            .ring
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(f)
    }

    /// Saves (at least) the last `seconds` to `path` on a background thread.
    pub fn save(&self, seconds: f64, path: PathBuf) -> SaveHandle {
        let result = Arc::new(Mutex::new(None));
        let handle = SaveHandle {
            result: Arc::clone(&result),
        };

        // Copy what we need now, so the recorder keeps running while the file
        // is written.
        let frames: Vec<EncodedFrame> = self
            .with_ring(|r| r.clip(seconds).into_iter().cloned().collect())
            .unwrap_or_default();
        let header = self
            .shared
            .header
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let stream = *self.shared.stream.lock().unwrap_or_else(|e| e.into_inner());

        let spawned = thread::Builder::new().name("replay-save".into()).spawn({
            let result = Arc::clone(&result);
            move || {
                let outcome = write_clip(&frames, header.as_deref(), stream, &path);
                *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(outcome);
            }
        });
        if spawned.is_err() {
            *result.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(Err("could not start the save".into()));
        }
        handle
    }
}

impl Drop for ReplayRecorder {
    fn drop(&mut self) {
        self.slot.set_tap(None);
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let deadline = Instant::now() + STOP_TIMEOUT;
            while !thread.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

/// Writes `frames` to an MP4 at `path`, on a thread of its own so Media
/// Foundation gets the multithreaded apartment it needs.
fn write_clip(
    frames: &[EncodedFrame],
    header: Option<&[u8]>,
    stream: Option<EncodeSettings>,
    path: &Path,
) -> Result<SavedClip, String> {
    let (Some(first), Some(last)) = (frames.first(), frames.last()) else {
        return Err("nothing has been recorded yet".into());
    };
    let Some(settings) = stream else {
        return Err("the recorder has not started a stream".into());
    };
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create the folder: {e}"))?;
    }
    let session = MfSession::start().map_err(|e| e.message())?;
    let mut remux =
        Mp4Remuxer::create(&session, path, &settings, header).map_err(|e| e.message())?;
    for frame in frames {
        remux.write(frame).map_err(|e| e.message())?;
    }
    remux.finish().map_err(|e| e.message())?;
    Ok(SavedClip {
        path: path.to_owned(),
        frames: frames.len(),
        seconds: (last.timestamp + last.duration - first.timestamp) as f64 / 1e7,
        bytes: std::fs::metadata(path).map_or(0, |m| m.len()),
    })
}

/// Recorder thread body.
fn run(tap: &FrameTap, shared: &Shared, stop: &AtomicBool, settings: ReplaySettings) {
    let Ok(session) = MfSession::start() else {
        shared.set_status(RecorderStatus::Failed(
            "Media Foundation is unavailable".into(),
        ));
        return;
    };
    let mut encoder: Option<(H264Encoder, EncodeSettings)> = None;
    let mut origin: Option<Instant> = None;
    let mut scratch: Vec<u8> = Vec::new();

    while !stop.load(Ordering::Relaxed) {
        let Some(frame) = tap.take(POLL) else {
            continue;
        };
        if frame.format != PixelFormat::Nv12 {
            shared.set_status(RecorderStatus::Unsupported(
                "only NV12 video can be recorded".into(),
            ));
            tap.recycle(frame);
            continue;
        }

        let wanted = EncodeSettings {
            width: frame.width,
            height: frame.height,
            fps: settings.fps,
            bitrate_bps: settings.bitrate_mbps.saturating_mul(1_000_000),
        };
        if encoder
            .as_ref()
            .is_none_or(|(_, current)| *current != wanted)
        {
            // A new stream (first frame, or the resolution changed).
            match H264Encoder::new(&session, &wanted) {
                Ok(e) => {
                    encoder = Some((e, wanted));
                    origin = Some(frame.arrived);
                    *shared.stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(wanted);
                    *shared.header.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    if let Some(ring) = shared
                        .ring
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_mut()
                    {
                        ring.clear();
                    }
                }
                Err(e) => {
                    shared.set_status(RecorderStatus::Unsupported(e.message()));
                    tap.recycle(frame);
                    thread::sleep(POLL);
                    continue;
                }
            }
        }
        let Some((encoder, _)) = encoder.as_mut() else {
            tap.recycle(frame);
            continue;
        };

        let timestamp = origin.map_or(0, |t| {
            frame.arrived.saturating_duration_since(t).as_nanos() / 100
        }) as i64;
        match encode_frame(encoder, &frame, timestamp, &mut scratch) {
            Ok(out) => {
                for encoded in out {
                    if encoded.keyframe
                        && shared
                            .header
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .is_none()
                    {
                        let header = encoder.output_type().ok().and_then(|t| sequence_header(&t));
                        *shared.header.lock().unwrap_or_else(|e| e.into_inner()) = header;
                    }
                    shared.encoded_frames.fetch_add(1, Ordering::Relaxed);
                    if let Some(ring) = shared
                        .ring
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .as_mut()
                    {
                        ring.push(encoded);
                    }
                }
                shared.set_status(RecorderStatus::Recording);
            }
            Err(e) => shared.set_status(RecorderStatus::Failed(e.message())),
        }
        tap.recycle(frame);
    }
    if let Some((mut encoder, _)) = encoder {
        let _ = encoder.flush();
    }
}

/// Encodes `frame`, repacking its rows first when the stride is padded.
fn encode_frame(
    encoder: &mut H264Encoder,
    frame: &Frame,
    timestamp: i64,
    scratch: &mut Vec<u8>,
) -> windows::core::Result<Vec<EncodedFrame>> {
    if frame.stride == frame.width {
        return encoder.encode(&frame.data, timestamp);
    }
    let (w, h, stride) = (
        frame.width as usize,
        frame.height as usize,
        frame.stride as usize,
    );
    scratch.clear();
    for row in 0..h {
        scratch.extend_from_slice(&frame.data[row * stride..row * stride + w]);
    }
    let uv = stride * h;
    for row in 0..h / 2 {
        scratch.extend_from_slice(&frame.data[uv + row * stride..uv + row * stride + w]);
    }
    encoder.encode(scratch, timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsrc::TestSource;

    /// Records a synthetic source; skips quietly on a PC without a hardware
    /// H.264 encoder, which is not a failure of the code under test.
    fn record(seconds: f64) -> Option<(ReplayRecorder, TestSource)> {
        let slot = Arc::new(FrameSlot::new());
        let source = TestSource::start(Arc::clone(&slot), 640, 360, 30);
        let recorder = ReplayRecorder::start(
            slot,
            ReplaySettings {
                keep_seconds: 10.0,
                bitrate_mbps: 4,
                fps: 30,
            },
        );
        thread::sleep(Duration::from_secs_f64(seconds));
        match recorder.status() {
            RecorderStatus::Unsupported(why) => {
                eprintln!("skipped: {why}");
                None
            }
            _ => Some((recorder, source)),
        }
    }

    #[test]
    fn it_records_and_saves_a_clip_that_is_longer_than_zero() {
        let Some((recorder, _source)) = record(3.5) else {
            return;
        };
        assert_eq!(recorder.status(), RecorderStatus::Recording);
        assert!(
            recorder.buffered_seconds() > 2.0,
            "{}",
            recorder.buffered_seconds()
        );
        assert!(recorder.buffered_bytes() > 0);

        let path = std::env::temp_dir().join(format!("dockview-rec-{}.mp4", std::process::id()));
        let saved = recorder
            .save(2.0, path.clone())
            .wait(Duration::from_secs(20))
            .expect("save finished")
            .expect("save succeeded");
        let _ = std::fs::remove_file(&path);
        assert!(saved.seconds >= 2.0, "{saved:?}");
        assert!(saved.bytes > 1_000 && saved.frames >= 55, "{saved:?}");
    }

    #[test]
    fn saving_before_anything_is_recorded_reports_it() {
        let slot = Arc::new(FrameSlot::new());
        let recorder = ReplayRecorder::start(slot, ReplaySettings::default());
        let result = recorder
            .save(5.0, std::env::temp_dir().join("dockview-never.mp4"))
            .wait(Duration::from_secs(5))
            .expect("finished");
        assert_eq!(result.unwrap_err(), "nothing has been recorded yet");
    }

    #[test]
    fn dropping_the_recorder_detaches_it_from_the_frame_stream() {
        let slot = Arc::new(FrameSlot::new());
        let recorder = ReplayRecorder::start(Arc::clone(&slot), ReplaySettings::default());
        drop(recorder);
        // Publishing afterwards must not panic or queue anywhere.
        slot.publish(crate::testsrc::nv12_bars(64, 36));
        assert_eq!(slot.published(), 1);
    }
}
