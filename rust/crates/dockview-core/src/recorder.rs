//! Live replay recording.
//!
//! A background thread taps every video frame, compresses it with the GPU's
//! H.264 encoder and keeps the newest footage in a [`ReplayRing`]. Saving cuts
//! a window out of the ring and writes it to an MP4 *without re-encoding*, on
//! its own short-lived thread so the live view is never held up.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::encode::{sequence_header, EncodeSettings, EncodedFrame, H264Encoder, Mp4Remuxer};
use crate::frame::{Frame, FrameSlot, FrameTap, PixelFormat};
use crate::mf::MfSession;
use crate::pcm::{frames_to_ticks, AudioTap, PcmRing};
use crate::recording::{
    audio_track_count, disk_free_bytes, disk_level, part_path, ActiveRecording, DiskLevel,
    EndReason, RecordingInfo, RecordingOutcome, RecordingState, DISK_START_MIN_BYTES,
    DISK_STOP_BYTES,
};
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

/// A request for the recording, picked up by the recorder thread.
enum RecCommand {
    Start(PathBuf),
    Stop,
}

struct Shared {
    /// Requests in order, so a quick start-then-stop is not lost.
    rec_commands: Mutex<VecDeque<RecCommand>>,
    rec_state: Mutex<RecordingState>,
    /// Footage written to the recording, as `f64` bits.
    rec_seconds: AtomicU64,
    rec_path: Mutex<Option<PathBuf>>,
    /// Outcome of the last recording, until the interface collects it.
    rec_result: Mutex<VecDeque<RecordingOutcome>>,
    /// The disk is running low (set by the recorder thread).
    rec_low_disk: AtomicBool,
    /// Free space below which a recording stops.
    disk_stop_bytes: AtomicU64,
    /// How long a start request waits for video, in milliseconds.
    no_signal_ms: AtomicU64,
    /// The zero of the shared timeline: video and audio are both stamped
    /// relative to this instant.
    epoch: Instant,
    pcm: Mutex<PcmRing>,
    /// The microphone, on the same timeline; empty while it is off.
    mic_pcm: Mutex<PcmRing>,
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
            rec_commands: Mutex::new(VecDeque::new()),
            rec_state: Mutex::new(RecordingState::Idle),
            rec_seconds: AtomicU64::new(0),
            rec_path: Mutex::new(None),
            rec_result: Mutex::new(VecDeque::new()),
            rec_low_disk: AtomicBool::new(false),
            disk_stop_bytes: AtomicU64::new(DISK_STOP_BYTES),
            no_signal_ms: AtomicU64::new(10_000),
            epoch: Instant::now(),
            pcm: Mutex::new(PcmRing::new(0.0)),
            mic_pcm: Mutex::new(PcmRing::new(0.0)),
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
    mic_tap: Arc<AudioTap>,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ReplayRecorder {
    /// Starts recording every frame published to `slot`.
    pub fn start(slot: Arc<FrameSlot>, settings: ReplaySettings) -> Self {
        let tap = Arc::new(FrameTap::new(TAP_CAPACITY));
        let audio_tap = Arc::new(AudioTap::new(AUDIO_TAP_CAPACITY));
        let mic_tap = Arc::new(AudioTap::new(AUDIO_TAP_CAPACITY));
        let shared = Arc::new(Shared::default());
        *shared.pcm.lock().unwrap_or_else(|e| e.into_inner()) = PcmRing::new(settings.keep_seconds);
        *shared.mic_pcm.lock().unwrap_or_else(|e| e.into_inner()) =
            PcmRing::new(settings.keep_seconds);
        *shared.ring.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(ReplayRing::new(settings.keep_seconds));
        shared.set_status(RecorderStatus::Starting);
        let stop = Arc::new(AtomicBool::new(false));

        let thread = {
            let (tap, audio, mic, shared, stop) = (
                Arc::clone(&tap),
                Arc::clone(&audio_tap),
                Arc::clone(&mic_tap),
                Arc::clone(&shared),
                Arc::clone(&stop),
            );
            thread::Builder::new()
                .name("replay-recorder".into())
                .spawn(move || run(&tap, &audio, &mic, &shared, &stop, settings))
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
            mic_tap,
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

    /// Where the microphone sends its audio.
    pub fn mic_tap(&self) -> Arc<AudioTap> {
        Arc::clone(&self.mic_tap)
    }

    /// Forgets everything the microphone recorded so far. Called when the
    /// microphone is turned off, so nothing it heard stays in the buffer.
    pub fn clear_mic(&self) {
        while self.mic_tap.take(Duration::ZERO).is_some() {}
        self.shared
            .mic_pcm
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
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
            .mic_pcm
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_keep_seconds(seconds);
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

    /// Starts recording the whole session to `path` (a fragmented MP4). The
    /// file begins at the next keyframe, within about a second.
    pub fn start_recording(&self, path: PathBuf) -> Result<(), String> {
        if self.status() != RecorderStatus::Recording {
            return Err("the recorder is not running yet".into());
        }
        if disk_free_bytes(&path).is_some_and(|free| free < DISK_START_MIN_BYTES) {
            return Err("not enough free disk space to record".into());
        }
        let mut state = self
            .shared
            .rec_state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if *state != RecordingState::Idle {
            return Err("already recording".into());
        }
        *state = RecordingState::Starting;
        self.shared
            .rec_result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.shared.rec_low_disk.store(false, Ordering::Relaxed);
        self.shared
            .rec_commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(RecCommand::Start(path));
        Ok(())
    }

    /// Ends the recording; the result arrives through
    /// [`ReplayRecorder::take_recording_result`].
    pub fn stop_recording(&self) {
        self.shared
            .rec_commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(RecCommand::Stop);
    }

    pub fn recording(&self) -> RecordingInfo {
        let state = *self
            .shared
            .rec_state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let bytes = self
            .shared
            .rec_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .filter(|_| state == RecordingState::Recording)
            .and_then(|p| std::fs::metadata(p).ok())
            .map_or(0, |m| m.len());
        RecordingInfo {
            state,
            seconds: f64::from_bits(self.shared.rec_seconds.load(Ordering::Relaxed)),
            bytes,
            low_disk: self.shared.rec_low_disk.load(Ordering::Relaxed),
        }
    }

    /// The next file of a recording that has ended, with why (whether you
    /// stopped it or it ended by itself). Call until `None`: a recording that
    /// changed resolution reports each of its files.
    pub fn take_recording_result(&self) -> Option<RecordingOutcome> {
        self.shared
            .rec_result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
    }

    /// Changes the free space below which a recording stops (for tests).
    pub fn set_disk_stop_bytes(&self, bytes: u64) {
        self.shared.disk_stop_bytes.store(bytes, Ordering::Relaxed);
    }

    /// Changes how long a start request waits for video (for tests).
    pub fn set_no_signal_timeout(&self, timeout: Duration) {
        self.shared
            .no_signal_ms
            .store(timeout.as_millis() as u64, Ordering::Relaxed);
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
        let (game, mic) = match (frames.first(), frames.last()) {
            (Some(first), Some(last)) => {
                let (from, to) = (first.timestamp, last.timestamp + last.duration);
                let game = {
                    let pcm = self.shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
                    (!pcm.is_empty()).then(|| pcm.extract(from, to))
                };
                // The microphone only counts if it was heard during this clip.
                let mic = {
                    let pcm = self
                        .shared
                        .mic_pcm
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    pcm.end_ts()
                        .is_some_and(|end| end > from)
                        .then(|| pcm.extract(from, to))
                };
                (game, mic)
            }
            _ => (None, None),
        };
        let tracks = audio_tracks(game, mic);
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
                let outcome = write_clip(&frames, &tracks, header.as_deref(), stream, &path);
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

/// The audio tracks of a clip, in file order.
///
/// With a microphone the clip carries three: the mix of both (what every player
/// plays by default), then the game on its own and the microphone on its own so
/// an editor can rebalance them. Without one, just the game.
fn audio_tracks(game: Option<Vec<i16>>, mic: Option<Vec<i16>>) -> Vec<Vec<i16>> {
    match (game, mic) {
        (Some(game), Some(mic)) => vec![crate::pcm::mix_tracks(&game, &mic), game, mic],
        (Some(only), None) | (None, Some(only)) => vec![only],
        (None, None) => Vec::new(),
    }
}

/// Writes `frames` to an MP4 at `path`, on a thread of its own so Media
/// Foundation gets the multithreaded apartment it needs.
fn write_clip(
    frames: &[EncodedFrame],
    tracks: &[Vec<i16>],
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
    let mut remux = Mp4Remuxer::create(&session, path, &settings, header, tracks.len(), false)
        .map_err(|e| e.message())?;

    // Interleave audio and video by time so the MP4 is written in order.
    const AUDIO_BLOCK_FRAMES: usize = 1024;
    let channels = crate::audio_sync::CHANNELS;
    let mut audio_at = vec![0usize; tracks.len()];
    let write_audio_until = |remux: &mut Mp4Remuxer, audio_at: &mut [usize], until: i64| {
        for (track, samples) in tracks.iter().enumerate() {
            let total = samples.len() / channels;
            while audio_at[track] < total && frames_to_ticks(audio_at[track] as i64) <= until {
                let start = audio_at[track];
                let end = (start + AUDIO_BLOCK_FRAMES).min(total);
                remux
                    .write_audio(
                        track,
                        frames_to_ticks(start as i64),
                        &samples[start * channels..end * channels],
                    )
                    .map_err(|e| e.message())?;
                audio_at[track] = end;
            }
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
    mic: &AudioTap,
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
    let mut rec = RecCtx::default();

    while !stop.load(Ordering::Relaxed) {
        drain_audio(audio, mic, shared);
        rec_commands(&mut rec, shared);
        rec_pump(&mut rec, shared);
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
            // A new stream (first frame, or the resolution changed). A file
            // cannot change size halfway, so a running recording ends here.
            if let Some(active) = rec.active.take() {
                rec_finish(active, shared, EndReason::ResolutionChanged, true);
                rec.part += 1;
                if let Some(base) = &rec.base {
                    rec.pending = Some(part_path(base, rec.part));
                    rec.pending_since = Some(Instant::now());
                }
            }
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
                    rec_on_frame(&mut rec, shared, &session, &wanted, &encoded);
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
    // Closing the program while recording still finishes the file.
    if let Some(active) = rec.active.take() {
        rec_finish(active, shared, EndReason::Stopped, false);
    } else if rec.pending.take().is_some() {
        rec_end(
            shared,
            Err("stopped before the first frame".into()),
            EndReason::Cancelled,
            false,
        );
    }
    if let Some((mut encoder, _)) = encoder {
        let _ = encoder.flush();
    }
}

/// How often the free disk space is checked while recording.
const DISK_CHECK_EVERY: Duration = Duration::from_secs(2);

/// The recording's state on the recorder thread.
struct RecCtx {
    /// Asked to start; waiting for a keyframe.
    pending: Option<PathBuf>,
    /// When the wait began, to give up if no video arrives.
    pending_since: Option<Instant>,
    active: Option<ActiveRecording>,
    /// The path the user asked for; later parts are named after it.
    base: Option<PathBuf>,
    /// Which file of the recording is being written (1 is the first).
    part: u32,
    last_disk_check: Instant,
}

impl Default for RecCtx {
    fn default() -> Self {
        Self {
            pending: None,
            pending_since: None,
            active: None,
            base: None,
            part: 1,
            last_disk_check: Instant::now(),
        }
    }
}

/// Ends the recording (or one of its files). With `continues`, the next file is
/// already waiting, so the state goes back to "starting" instead of idle.
fn rec_end(shared: &Shared, result: Result<SavedClip, String>, reason: EndReason, continues: bool) {
    *shared.rec_state.lock().unwrap_or_else(|e| e.into_inner()) = if continues {
        RecordingState::Starting
    } else {
        RecordingState::Idle
    };
    shared.rec_seconds.store(0f64.to_bits(), Ordering::Relaxed);
    shared
        .rec_result
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push_back(RecordingOutcome {
            result,
            reason,
            continues,
        });
}

fn rec_finish(active: ActiveRecording, shared: &Shared, reason: EndReason, continues: bool) {
    let result = {
        let game = shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
        let mic = shared.mic_pcm.lock().unwrap_or_else(|e| e.into_inner());
        active.finish(&game, &mic)
    };
    let reason = if result.is_err() {
        EndReason::Failed
    } else {
        reason
    };
    rec_end(shared, result, reason, continues);
}

/// Applies the start and stop requests, in order.
fn rec_commands(rec: &mut RecCtx, shared: &Shared) {
    let queued: Vec<RecCommand> = shared
        .rec_commands
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain(..)
        .collect();
    for command in queued {
        match command {
            RecCommand::Start(path) if rec.active.is_none() && rec.pending.is_none() => {
                rec.base = Some(path.clone());
                rec.part = 1;
                rec.pending = Some(path);
                rec.pending_since = Some(Instant::now());
            }
            RecCommand::Start(_) => {}
            RecCommand::Stop => {
                if let Some(active) = rec.active.take() {
                    rec_finish(active, shared, EndReason::Stopped, false);
                } else if rec.pending.take().is_some() {
                    rec_end(
                        shared,
                        Err("stopped before the first frame".into()),
                        EndReason::Cancelled,
                        false,
                    );
                }
            }
        }
    }
}

/// Writes the audio that has arrived to the recording, and watches the disk
/// and the video signal.
fn rec_pump(rec: &mut RecCtx, shared: &Shared) {
    // Give up on a start that never gets video.
    if let (Some(_), Some(since)) = (&rec.pending, rec.pending_since) {
        let wait = Duration::from_millis(shared.no_signal_ms.load(Ordering::Relaxed));
        if since.elapsed() > wait {
            rec.pending = None;
            rec_end(
                shared,
                Err("no video signal arrived".into()),
                EndReason::NoSignal,
                false,
            );
        }
    }
    let Some(active) = rec.active.as_mut() else {
        return;
    };
    let result = {
        let game = shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
        let mic = shared.mic_pcm.lock().unwrap_or_else(|e| e.into_inner());
        active.pump_audio(&game, &mic, false)
    };
    if let Err(e) = result {
        // The file stays valid up to the last fragment; just stop writing.
        rec.active = None;
        rec_end(shared, Err(e), EndReason::Failed, false);
        return;
    }
    if rec.last_disk_check.elapsed() >= DISK_CHECK_EVERY {
        rec.last_disk_check = Instant::now();
        let stop = shared.disk_stop_bytes.load(Ordering::Relaxed);
        match disk_free_bytes(active.path()).map(|free| disk_level(free, stop)) {
            Some(DiskLevel::Full) => {
                if let Some(active) = rec.active.take() {
                    rec_finish(active, shared, EndReason::DiskFull, false);
                }
            }
            Some(DiskLevel::Low) => shared.rec_low_disk.store(true, Ordering::Relaxed),
            _ => {}
        }
    }
}

/// Gives a freshly compressed frame to the recording: starts the file at the
/// first keyframe after the request, then writes every frame.
fn rec_on_frame(
    rec: &mut RecCtx,
    shared: &Shared,
    session: &MfSession,
    settings: &EncodeSettings,
    encoded: &EncodedFrame,
) {
    if let Some(active) = rec.active.as_mut() {
        match active.write_frame(encoded) {
            Ok(()) => shared
                .rec_seconds
                .store(active.seconds().to_bits(), Ordering::Relaxed),
            Err(e) => {
                rec.active = None;
                rec_end(shared, Err(e), EndReason::Failed, false);
            }
        }
    } else if rec.pending.is_some() && encoded.keyframe {
        let Some(path) = rec.pending.take() else {
            return;
        };
        let header = shared
            .header
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let tracks = {
            let game = shared.pcm.lock().unwrap_or_else(|e| e.into_inner());
            let mic = shared.mic_pcm.lock().unwrap_or_else(|e| e.into_inner());
            audio_track_count(!game.is_empty(), !mic.is_empty())
        };
        match ActiveRecording::begin(session, &path, settings, header.as_deref(), tracks, encoded) {
            Ok(active) => {
                *shared.rec_path.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(active.path().to_owned());
                *shared.rec_state.lock().unwrap_or_else(|e| e.into_inner()) =
                    RecordingState::Recording;
                rec.pending_since = None;
                rec.active = Some(active);
            }
            Err(e) => rec_end(shared, Err(e), EndReason::Failed, false),
        }
    }
}

/// Moves every queued audio block onto the shared timeline.
fn drain_audio(audio: &AudioTap, mic: &AudioTap, shared: &Shared) {
    let to_ticks =
        |start: Instant| (start.saturating_duration_since(shared.epoch).as_nanos() / 100) as i64;
    {
        let mut pcm = shared.mic_pcm.lock().unwrap_or_else(|e| e.into_inner());
        while let Some(chunk) = mic.take(Duration::ZERO) {
            pcm.push(to_ticks(chunk.start), &chunk.samples);
        }
    }
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
        use windows::Win32::Media::MediaFoundation::MF_SOURCE_READER_FIRST_AUDIO_STREAM;
        audio_track_summary(path, MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32)
    }

    /// Length in seconds and RMS level of the file's stream number `stream`
    /// (0 is the video; audio tracks follow in file order).
    fn audio_track_summary(path: &Path, stream: u32) -> Option<(f64, f64)> {
        use windows::core::HSTRING;
        use windows::Win32::Media::MediaFoundation::{
            IMFAttributes, IMFSample, MFAudioFormat_PCM, MFCreateMediaType,
            MFCreateSourceReaderFromURL, MFMediaType_Audio, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
            MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
            MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
            MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ALL_STREAMS,
        };
        let _session = MfSession::start().ok()?;
        // SAFETY: plain source-reader calls on a file this test just wrote.
        unsafe {
            let reader = MFCreateSourceReaderFromURL(
                &HSTRING::from(path.as_os_str()),
                None::<&IMFAttributes>,
            )
            .ok()?;
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
            reader
                .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
                .ok()?;
            reader.SetStreamSelection(stream, true).ok()?;
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

    /// Feeds a sine of the given amplitude and pitch into `tap` in real time.
    fn feed_sine(
        tap: Arc<AudioTap>,
        amplitude: f32,
        hz: f32,
        stop: Arc<AtomicBool>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut phase = 0.0f32;
            while !stop.load(Ordering::Relaxed) {
                let mut block = Vec::with_capacity(960);
                for _ in 0..480 {
                    let v = amplitude * (phase * std::f32::consts::TAU).sin();
                    block.extend_from_slice(&[v, v]);
                    phase = (phase + hz / 48_000.0).fract();
                }
                tap.offer(Instant::now(), &block);
                thread::sleep(Duration::from_millis(10));
            }
        })
    }

    #[test]
    fn a_clip_with_a_microphone_has_mix_game_and_mic_tracks() {
        let Some((recorder, _source)) = record(0.2) else {
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        let feeders = [
            feed_sine(recorder.audio_tap(), 0.5, 440.0, Arc::clone(&stop)),
            feed_sine(recorder.mic_tap(), 0.25, 880.0, Arc::clone(&stop)),
        ];
        thread::sleep(Duration::from_secs(4));

        let path =
            std::env::temp_dir().join(format!("dockview-rec-mic-{}.mp4", std::process::id()));
        let saved = recorder
            .save(2.0, path.clone())
            .wait(Duration::from_secs(20))
            .expect("save finished")
            .expect("save succeeded");
        let tracks: Vec<_> = (0..=4).map(|n| audio_track_summary(&path, n)).collect();
        let fourth = audio_track_summary(&path, 3); // the video, not audio

        // Forgetting the microphone drops its track from the next clip.
        recorder.clear_mic();
        stop.store(true, Ordering::Relaxed);
        for f in feeders {
            let _ = f.join();
        }
        recorder.clear_mic();
        let path2 =
            std::env::temp_dir().join(format!("dockview-rec-nomic-{}.mp4", std::process::id()));
        recorder
            .save(2.0, path2.clone())
            .wait(Duration::from_secs(20))
            .expect("save finished")
            .expect("save succeeded");
        let second = audio_track_summary(&path2, 2);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&path2);

        // Media Foundation's reader lists the audio tracks last-added first (the
        // file itself is video, mix, game, mic) and the video after them.
        let [mic, game, mix] = [tracks[0], tracks[1], tracks[2]].map(|t| t.expect("track decodes"));
        for (name, (seconds, _)) in [("mix", mix), ("game", game), ("mic", mic)] {
            assert!(
                (seconds - saved.seconds).abs() < 0.25,
                "{name} {seconds:.2}s vs video {:.2}s",
                saved.seconds
            );
        }
        // Sines of 0.5 and 0.25 have RMS 0.354 and 0.177; the mix is their power sum.
        assert!((0.33..0.46).contains(&mix.1), "mix RMS {:.3}", mix.1);
        assert!((0.28..0.45).contains(&game.1), "game RMS {:.3}", game.1);
        assert!((0.13..0.23).contains(&mic.1), "mic RMS {:.3}", mic.1);
        assert!(fourth.is_none(), "exactly three audio tracks");
        assert!(second.is_none(), "no microphone, no second audio track");
    }

    fn wait_for_outcome(recorder: &ReplayRecorder) -> RecordingOutcome {
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            if let Some(outcome) = recorder.take_recording_result() {
                return outcome;
            }
            assert!(Instant::now() < deadline, "the recording never finished");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_for_recording_result(recorder: &ReplayRecorder) -> Result<SavedClip, String> {
        wait_for_outcome(recorder).result
    }

    /// A recorder fed by a test source, keeping the slot so the source can be
    /// swapped for another (to simulate a signal that drops or changes size).
    fn recorder_with_slot() -> Option<(ReplayRecorder, Arc<FrameSlot>, TestSource)> {
        let slot = Arc::new(FrameSlot::new());
        let source = TestSource::start(Arc::clone(&slot), 640, 360, 30);
        let recorder = ReplayRecorder::start(
            Arc::clone(&slot),
            ReplaySettings {
                keep_seconds: 10.0,
                bitrate_mbps: 4,
                fps: 30,
            },
        );
        let ready = Instant::now() + Duration::from_secs(5);
        while recorder.status() == RecorderStatus::Starting && Instant::now() < ready {
            thread::sleep(Duration::from_millis(20));
        }
        match recorder.status() {
            RecorderStatus::Recording => Some((recorder, slot, source)),
            other => {
                eprintln!("skipped: {other:?}");
                None
            }
        }
    }

    /// Number of video samples a file holds and where the last one ends.
    fn video_samples(path: &Path) -> Option<(u64, f64)> {
        use windows::core::HSTRING;
        use windows::Win32::Media::MediaFoundation::{
            IMFAttributes, IMFSample, MFCreateSourceReaderFromURL, MF_SOURCE_READERF_ENDOFSTREAM,
            MF_SOURCE_READER_FIRST_VIDEO_STREAM,
        };
        let _session = MfSession::start().ok()?;
        // SAFETY: plain source-reader calls on a file this test just wrote.
        unsafe {
            let reader = MFCreateSourceReaderFromURL(
                &HSTRING::from(path.as_os_str()),
                None::<&IMFAttributes>,
            )
            .ok()?;
            let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            let (mut count, mut end) = (0u64, 0i64);
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
                if let Some(sample) = sample {
                    count += 1;
                    end = ts + sample.GetSampleDuration().unwrap_or(0);
                }
            }
            Some((count, end as f64 / 1e7))
        }
    }

    #[test]
    fn a_resolution_change_closes_the_file_and_goes_on_in_a_new_one() {
        let Some((recorder, slot, source)) = recorder_with_slot() else {
            return;
        };
        let path = std::env::temp_dir().join(format!("dockview-parts-{}.mp4", std::process::id()));
        let second_path = part_path(&path, 2);
        recorder.start_recording(path.clone()).expect("starts");
        thread::sleep(Duration::from_secs(6));
        drop(source);
        let _bigger = TestSource::start(Arc::clone(&slot), 800, 448, 30);
        thread::sleep(Duration::from_secs(6));
        recorder.stop_recording();

        let first = wait_for_outcome(&recorder);
        let second = wait_for_outcome(&recorder);
        let first_video = first
            .result
            .as_ref()
            .ok()
            .and_then(|c| video_samples(&c.path));
        let second_video = second
            .result
            .as_ref()
            .ok()
            .and_then(|c| video_samples(&c.path));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&second_path);

        assert_eq!(first.reason, EndReason::ResolutionChanged);
        assert!(first.continues, "recording goes on in a second file");
        let first_clip = first.result.expect("the first file was saved");
        assert!(first_clip.seconds > 1.0, "{}", first_clip.seconds);
        assert_eq!(first_clip.path, path);

        assert_eq!(second.reason, EndReason::Stopped);
        assert!(!second.continues);
        let second_clip = second.result.expect("the second file was saved");
        assert!(second_clip.seconds > 1.0, "{}", second_clip.seconds);
        assert_eq!(second_clip.path, second_path);

        assert!(first_video.is_some_and(|(n, _)| n > 30), "{first_video:?}");
        assert!(
            second_video.is_some_and(|(n, _)| n > 30),
            "{second_video:?}"
        );
        assert_eq!(recorder.recording().state, RecordingState::Idle);
    }

    #[test]
    fn a_signal_that_drops_and_returns_stays_one_valid_file() {
        let Some((recorder, slot, source)) = recorder_with_slot() else {
            return;
        };
        let path = std::env::temp_dir().join(format!("dockview-gap-{}.mp4", std::process::id()));
        recorder.start_recording(path.clone()).expect("starts");
        thread::sleep(Duration::from_secs(4));
        drop(source); // the capture card goes away...
        thread::sleep(Duration::from_secs(3));
        assert_eq!(
            recorder.recording().state,
            RecordingState::Recording,
            "a short outage does not end the recording"
        );
        let _back = TestSource::start(Arc::clone(&slot), 640, 360, 30); // ...and returns
        thread::sleep(Duration::from_secs(4));
        recorder.stop_recording();

        let outcome = wait_for_outcome(&recorder);
        let clip = outcome.result.expect("saved");
        let video = video_samples(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(outcome.reason, EndReason::Stopped);
        // About 4 s + the 3 s outage + 4 s (less the wait for the first keyframe):
        // the outage is part of the timeline.
        assert!((6.5..12.0).contains(&clip.seconds), "{}", clip.seconds);
        let (samples, end) = video.expect("the file reads");
        assert!(samples >= 100, "{samples} samples");
        assert!(
            (end - clip.seconds).abs() < 0.5,
            "{end} vs {}",
            clip.seconds
        );
    }

    #[test]
    fn asking_to_record_without_video_gives_up_with_no_signal() {
        let Some((recorder, _slot, source)) = recorder_with_slot() else {
            return;
        };
        drop(source); // no video from now on
        recorder.set_no_signal_timeout(Duration::from_secs(1));
        let path = std::env::temp_dir().join(format!("dockview-nosig-{}.mp4", std::process::id()));
        recorder.start_recording(path.clone()).expect("starts");
        let outcome = wait_for_outcome(&recorder);
        let _ = std::fs::remove_file(&path);
        assert_eq!(outcome.reason, EndReason::NoSignal);
        assert!(outcome.result.is_err());
        assert!(!outcome.continues);
        assert_eq!(recorder.recording().state, RecordingState::Idle);
    }

    #[test]
    fn a_recording_stops_cleanly_when_the_disk_is_about_to_fill() {
        let Some((recorder, _slot, _source)) = recorder_with_slot() else {
            return;
        };
        let path = std::env::temp_dir().join(format!("dockview-full-{}.mp4", std::process::id()));
        recorder.start_recording(path.clone()).expect("starts");
        thread::sleep(Duration::from_secs(2));
        // Pretend the disk needs more free space than it has.
        recorder.set_disk_stop_bytes(u64::MAX);
        let outcome = wait_for_outcome(&recorder);
        let video = video_samples(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(outcome.reason, EndReason::DiskFull);
        let clip = outcome.result.expect("what was recorded is saved");
        assert!(clip.seconds > 1.0, "{}", clip.seconds);
        assert!(video.is_some_and(|(n, _)| n > 30), "{video:?}");
        assert_eq!(recorder.recording().state, RecordingState::Idle);
    }

    #[test]
    fn a_recording_is_a_fragmented_file_with_video_and_three_audio_tracks() {
        let Some((recorder, _source)) = record(0.2) else {
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        let feeders = [
            feed_sine(recorder.audio_tap(), 0.5, 440.0, Arc::clone(&stop)),
            feed_sine(recorder.mic_tap(), 0.25, 880.0, Arc::clone(&stop)),
        ];
        thread::sleep(Duration::from_secs(2)); // let audio reach the rings first

        let path =
            std::env::temp_dir().join(format!("dockview-recording-{}.mp4", std::process::id()));
        recorder.start_recording(path.clone()).expect("starts");
        assert!(
            recorder.start_recording(path.clone()).is_err(),
            "a second start is refused"
        );
        thread::sleep(Duration::from_secs(5));
        let info = recorder.recording();
        assert_eq!(info.state, RecordingState::Recording);
        assert!(info.seconds > 2.5, "{info:?}");
        assert!(info.bytes > 0, "the file grows while recording");

        recorder.stop_recording();
        let saved = wait_for_recording_result(&recorder).expect("the recording was saved");
        stop.store(true, Ordering::Relaxed);
        for f in feeders {
            let _ = f.join();
        }
        assert_eq!(recorder.recording().state, RecordingState::Idle);

        let bytes = std::fs::read(&path).expect("the file exists");
        let fragmented = bytes.windows(4).any(|w| w == b"moof");
        let tracks: Vec<_> = (0..3).map(|n| audio_track_summary(&path, n)).collect();
        let _ = std::fs::remove_file(&path);

        assert!(fragmented, "the file must be a fragmented MP4");
        assert!((4.0..7.5).contains(&saved.seconds), "{}", saved.seconds);
        // The reader lists the tracks last-added first: mic, game, mix.
        let [mic, game, mix] = [tracks[0], tracks[1], tracks[2]].map(|t| t.expect("track decodes"));
        for (name, (seconds, _)) in [("mix", mix), ("game", game), ("mic", mic)] {
            assert!(
                (seconds - saved.seconds).abs() < 0.3,
                "{name} {seconds:.2}s vs video {:.2}s",
                saved.seconds
            );
        }
        assert!((0.33..0.46).contains(&mix.1), "mix RMS {:.3}", mix.1);
        assert!((0.28..0.45).contains(&game.1), "game RMS {:.3}", game.1);
        assert!((0.13..0.23).contains(&mic.1), "mic RMS {:.3}", mic.1);
    }

    #[test]
    fn a_recording_cut_off_halfway_still_plays() {
        let Some((recorder, _source)) = record(0.2) else {
            return;
        };
        let stop = Arc::new(AtomicBool::new(false));
        let feeder = feed_sine(recorder.audio_tap(), 0.5, 440.0, Arc::clone(&stop));
        thread::sleep(Duration::from_secs(2));

        let path = std::env::temp_dir().join(format!("dockview-cutoff-{}.mp4", std::process::id()));
        let copy =
            std::env::temp_dir().join(format!("dockview-cutoff-copy-{}.mp4", std::process::id()));
        recorder.start_recording(path.clone()).expect("starts");
        thread::sleep(Duration::from_secs(6));
        // What is on disk right now is what a crash would leave behind.
        std::fs::copy(&path, &copy).expect("copies the unfinished file");
        recorder.stop_recording();
        let _ = wait_for_recording_result(&recorder);
        stop.store(true, Ordering::Relaxed);
        let _ = feeder.join();

        let left_behind = audio_track_summary(&copy, 0);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&copy);
        let (seconds, _) = left_behind.expect("the unfinished file still opens and decodes");
        assert!(seconds > 1.5, "only {seconds:.2}s survived of about 6");
    }

    #[test]
    fn a_recording_asked_for_but_stopped_at_once_reports_an_error() {
        let Some((recorder, _source)) = record(0.2) else {
            return;
        };
        let ready = Instant::now() + Duration::from_secs(5);
        while recorder.status() != RecorderStatus::Recording && Instant::now() < ready {
            thread::sleep(Duration::from_millis(20));
        }
        let path = std::env::temp_dir().join(format!("dockview-never-{}.mp4", std::process::id()));
        recorder.start_recording(path.clone()).expect("starts");
        recorder.stop_recording();
        // Either it never began (a quiet cancel) or a keyframe was so close
        // that it did.
        let outcome = wait_for_outcome(&recorder);
        let _ = std::fs::remove_file(&path);
        if let Err(e) = &outcome.result {
            assert!(e.contains("before the first frame"), "{e}");
            assert_eq!(outcome.reason, EndReason::Cancelled);
        }
        assert_eq!(recorder.recording().state, RecordingState::Idle);
    }

    #[test]
    fn tracks_are_the_mix_then_game_then_mic_when_there_is_a_microphone() {
        let game = vec![100i16, 200];
        let mic = vec![1i16, 2];
        let tracks = audio_tracks(Some(game.clone()), Some(mic.clone()));
        assert_eq!(tracks, vec![vec![101, 202], game.clone(), mic.clone()]);
        assert_eq!(audio_tracks(Some(game.clone()), None), vec![game]);
        assert_eq!(audio_tracks(None, Some(mic.clone())), vec![mic]);
        assert!(audio_tracks(None, None).is_empty());
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
