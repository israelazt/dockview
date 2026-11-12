//! Live replay recording.
//!
//! A background thread taps every video frame, compresses it with the GPU's
//! H.264 encoder and keeps the newest footage in a [`ReplayRing`]. Saving cuts
//! a window out of the ring and writes it to an MP4 *without re-encoding*, on
//! its own short-lived thread so the live view is never held up.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::encode::{sequence_header, EncodeSettings, EncodedFrame, H264Encoder, Mp4Remuxer};
use crate::frame::{Frame, FrameSlot, FrameTap, PixelFormat};
use crate::mf::MfSession;
use crate::pcm::{frames_to_ticks, AudioTap, PcmRing};
use crate::replay::ReplayRing;

/// Frames the tap may hold before it starts dropping (about 130 ms at 60 fps).
const TAP_CAPACITY: usize = 8;
/// Audio blocks the tap may hold (about 640 ms of 10 ms blocks).
const AUDIO_TAP_CAPACITY: usize = 64;
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

struct Shared {
    /// The zero of the shared timeline: video and audio are both stamped
    /// relative to this instant.
    epoch: Instant,
    pcm: Mutex<PcmRing>,
    audio_offset_ms: AtomicI32,
    ring: Mutex<Option<ReplayRing>>,
    status: Mutex<Option<RecorderStatus>>,
    /// SPS/PPS of the stream, needed to build the MP4.
    header: Mutex<Option<Vec<u8>>>,
    /// Size and nominal settings of the stream being recorded.
    stream: Mutex<Option<EncodeSettings>>,
    encoded_frames: AtomicU64,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            pcm: Mutex::new(PcmRing::new(0.0)),
            audio_offset_ms: AtomicI32::new(0),
            ring: Mutex::new(None),
            status: Mutex::new(None),
            header: Mutex::new(None),
            stream: Mutex::new(None),
            encoded_frames: AtomicU64::new(0),
        }
    }
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
    audio_tap: Arc<AudioTap>,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ReplayRecorder {
    /// Starts recording every frame published to `slot`.
    pub fn start(slot: Arc<FrameSlot>, settings: ReplaySettings) -> Self {
        let tap = Arc::new(FrameTap::new(TAP_CAPACITY));
        let audio_tap = Arc::new(AudioTap::new(AUDIO_TAP_CAPACITY));
        let shared = Arc::new(Shared::default());
        *shared.pcm.lock().unwrap_or_else(|e| e.into_inner()) = PcmRing::new(settings.keep_seconds);
        *shared.ring.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(ReplayRing::new(settings.keep_seconds));
        shared.set_status(RecorderStatus::Starting);
        let stop = Arc::new(AtomicBool::new(false));

        let thread = {
            let (tap, audio, shared, stop) = (
                Arc::clone(&tap),
                Arc::clone(&audio_tap),
                Arc::clone(&shared),
                Arc::clone(&stop),
            );
            thread::Builder::new()
                .name("replay-recorder".into())
                .spawn(move || run(&tap, &audio, &shared, &stop, settings))
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
            audio_tap,
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

    /// The queue the audio engine should feed (see `AudioEngine::set_tap`).
    pub fn audio_tap(&self) -> Arc<AudioTap> {
        Arc::clone(&self.audio_tap)
    }

    /// Shifts the recorded audio relative to the video, like the live sync
    /// control (positive delays the audio).
    pub fn set_audio_offset_ms(&self, ms: i32) {
        self.shared.audio_offset_ms.store(ms, Ordering::Relaxed);
    }

    /// Seconds of audio held right now.
    pub fn buffered_audio_seconds(&self) -> f64 {
        self.shared
            .pcm
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .duration_seconds()
    }

    /// Changes how much footage is kept.
    pub fn set_keep_seconds(&self, seconds: f64) {
        self.shared
            .pcm
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_keep_seconds(seconds);
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
        let audio = match (frames.first(), frames.last()) {
            (Some(first), Some(last)) => {
                let pcm = self.shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
                (!pcm.is_empty())
                    .then(|| pcm.extract(first.timestamp, last.timestamp + last.duration))
            }
            _ => None,
        };
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
                let outcome =
                    write_clip(&frames, audio.as_deref(), header.as_deref(), stream, &path);
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
    audio: Option<&[i16]>,
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
    let mut remux = Mp4Remuxer::create(&session, path, &settings, header, audio.is_some())
        .map_err(|e| e.message())?;

    // Interleave audio and video by time so the MP4 is written in order.
    const AUDIO_BLOCK_FRAMES: usize = 1024;
    let channels = crate::audio_sync::CHANNELS;
    let audio = audio.unwrap_or(&[]);
    let audio_frames = audio.len() / channels;
    let mut audio_at = 0usize;
    let write_audio_until = |remux: &mut Mp4Remuxer, audio_at: &mut usize, until: i64| {
        while *audio_at < audio_frames && frames_to_ticks(*audio_at as i64) <= until {
            let end = (*audio_at + AUDIO_BLOCK_FRAMES).min(audio_frames);
            let block = &audio[*audio_at * channels..end * channels];
            remux
                .write_audio(frames_to_ticks(*audio_at as i64), block)
                .map_err(|e| e.message())?;
            *audio_at = end;
        }
        Ok::<(), String>(())
    };
    for frame in frames {
        write_audio_until(&mut remux, &mut audio_at, frame.timestamp - first.timestamp)?;
        remux.write(frame).map_err(|e| e.message())?;
    }
    write_audio_until(&mut remux, &mut audio_at, i64::MAX)?;
    remux.finish().map_err(|e| e.message())?;
    Ok(SavedClip {
        path: path.to_owned(),
        frames: frames.len(),
        seconds: (last.timestamp + last.duration - first.timestamp) as f64 / 1e7,
        bytes: std::fs::metadata(path).map_or(0, |m| m.len()),
    })
}

/// Recorder thread body.
fn run(
    tap: &FrameTap,
    audio: &AudioTap,
    shared: &Shared,
    stop: &AtomicBool,
    settings: ReplaySettings,
) {
    let Ok(session) = MfSession::start() else {
        shared.set_status(RecorderStatus::Failed(
            "Media Foundation is unavailable".into(),
        ));
        return;
    };
    let mut encoder: Option<(H264Encoder, EncodeSettings)> = None;
    let mut scratch: Vec<u8> = Vec::new();

    while !stop.load(Ordering::Relaxed) {
        drain_audio(audio, shared);
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

        let timestamp = (frame
            .arrived
            .saturating_duration_since(shared.epoch)
            .as_nanos()
            / 100) as i64;
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

/// Moves every queued audio block onto the shared timeline.
fn drain_audio(audio: &AudioTap, shared: &Shared) {
    let offset = i64::from(shared.audio_offset_ms.load(Ordering::Relaxed)) * 10_000;
    let mut pcm = shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
    while let Some(chunk) = audio.take(Duration::ZERO) {
        let start = (chunk
            .start
            .saturating_duration_since(shared.epoch)
            .as_nanos()
            / 100) as i64;
        pcm.push(start + offset, &chunk.samples);
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

    /// Decodes the first audio stream of an MP4: (seconds, RMS level 0..1).
    fn audio_summary(path: &Path) -> Option<(f64, f64)> {
        use windows::core::HSTRING;
        use windows::Win32::Media::MediaFoundation::{
            IMFAttributes, IMFSample, MFAudioFormat_PCM, MFCreateMediaType,
            MFCreateSourceReaderFromURL, MFMediaType_Audio, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
            MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
            MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
            MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
        };
        let _session = MfSession::start().ok()?;
        // SAFETY: plain source-reader calls on a file this test just wrote.
        unsafe {
            let reader = MFCreateSourceReaderFromURL(
                &HSTRING::from(path.as_os_str()),
                None::<&IMFAttributes>,
            )
            .ok()?;
            let stream = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
            let pcm = MFCreateMediaType().ok()?;
            pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).ok()?;
            pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)
                .ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4).ok()?;
            pcm.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192_000)
                .ok()?;
            reader.SetCurrentMediaType(stream, None, &pcm).ok()?;

            let (mut frames, mut sum_squares) = (0u64, 0f64);
            loop {
                let (mut flags, mut ts) = (0u32, 0i64);
                let mut sample: Option<IMFSample> = None;
                reader
                    .ReadSample(
                        stream,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut ts),
                        Some(&mut sample),
                    )
                    .ok()?;
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
                let Some(sample) = sample else { continue };
                let buffer = sample.ConvertToContiguousBuffer().ok()?;
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut len = 0u32;
                buffer.Lock(&mut data, None, Some(&mut len)).ok()?;
                let bytes = std::slice::from_raw_parts(data, len as usize);
                let (pairs, _) = bytes.as_chunks::<2>();
                for pair in pairs {
                    let v = f64::from(i16::from_le_bytes(*pair)) / 32768.0;
                    sum_squares += v * v;
                }
                frames += (len / 4) as u64;
                buffer.Unlock().ok()?;
            }
            (frames > 0).then(|| {
                (
                    frames as f64 / 48_000.0,
                    (sum_squares / (frames as f64 * 2.0)).sqrt(),
                )
            })
        }
    }

    #[test]
    fn a_saved_clip_carries_the_recorded_audio_at_the_right_length_and_level() {
        let Some((recorder, _source)) = record(0.2) else {
            return;
        };
        // Feed a 440 Hz sine at half scale, in 10 ms blocks, in real time.
        let tap = recorder.audio_tap();
        let stop = Arc::new(AtomicBool::new(false));
        let feeder = {
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                let mut phase = 0.0f32;
                while !stop.load(Ordering::Relaxed) {
                    let mut block = Vec::with_capacity(960);
                    for _ in 0..480 {
                        let v = 0.5 * (phase * std::f32::consts::TAU).sin();
                        block.extend_from_slice(&[v, v]);
                        phase = (phase + 440.0 / 48_000.0).fract();
                    }
                    tap.offer(Instant::now(), &block);
                    thread::sleep(Duration::from_millis(10));
                }
            })
        };
        thread::sleep(Duration::from_secs(4));
        assert!(
            recorder.buffered_audio_seconds() > 2.5,
            "{}",
            recorder.buffered_audio_seconds()
        );

        let path = std::env::temp_dir().join(format!("dockview-rec-av-{}.mp4", std::process::id()));
        let saved = recorder
            .save(2.0, path.clone())
            .wait(Duration::from_secs(20))
            .expect("save finished")
            .expect("save succeeded");
        stop.store(true, Ordering::Relaxed);
        let _ = feeder.join();

        let summary = audio_summary(&path);
        let _ = std::fs::remove_file(&path);
        let (seconds, rms) = summary.expect("the clip has a decodable audio track");
        assert!(
            (seconds - saved.seconds).abs() < 0.25,
            "audio {seconds:.2}s vs video {:.2}s",
            saved.seconds
        );
        // A sine of amplitude 0.5 has an RMS of about 0.354; AAC is lossy.
        assert!((0.28..0.45).contains(&rms), "RMS {rms:.3}");
    }

    #[test]
    fn a_clip_without_audio_is_still_valid() {
        let Some((recorder, _source)) = record(3.0) else {
            return;
        };
        let path = std::env::temp_dir().join(format!("dockview-rec-v-{}.mp4", std::process::id()));
        let saved = recorder
            .save(2.0, path.clone())
            .wait(Duration::from_secs(20))
            .unwrap()
            .unwrap();
        let has_audio = audio_summary(&path).is_some();
        let _ = std::fs::remove_file(&path);
        assert!(saved.frames > 0);
        assert!(!has_audio, "no audio was fed, so there is no audio track");
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
