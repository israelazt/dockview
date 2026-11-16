//! Live replay recording.
//!
//! A background thread taps every video frame, compresses it with the GPU's
//! H.264 encoder and keeps the newest footage in a [`ReplayRing`]. Saving cuts
//! a window out of the ring and writes it to an MP4 *without re-encoding*, on
//! its own short-lived thread so the live view is never held up.

mod clip;
mod runner;
mod session;
#[cfg(test)]
mod tests;

use clip::{audio_tracks, write_clip};
use runner::run;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::encode::{sequence_header, EncodeSettings, H264Encoder, Mp4Remuxer};
use crate::mf::MfSession;
use crate::recording::{disk_free_bytes, ActiveRecording};
use crate::worker::Worker;
use dockview_domain::encoded::EncodedFrame;
use dockview_domain::frame::{Frame, FrameSlot, FrameTap, PixelFormat};
use dockview_domain::pcm::{frames_to_ticks, AudioTap, PcmRing};
use dockview_domain::recording::{
    audio_track_count, disk_level, part_path, DiskLevel, EndReason, RecordingInfo,
    RecordingOutcome, RecordingState, DISK_START_MIN_BYTES, DISK_STOP_BYTES,
};
use dockview_domain::recording::{RecorderStatus, ReplaySettings, SaveHandle, SavedClip};
use dockview_domain::replay::ReplayRing;

/// Frames the tap may hold before it starts dropping (about 130 ms at 60 fps).
const TAP_CAPACITY: usize = 8;
/// Audio blocks the tap may hold (about 640 ms of 10 ms blocks).
const AUDIO_TAP_CAPACITY: usize = 64;
/// How long the recorder waits for a frame before checking for a stop request.
const POLL: Duration = Duration::from_millis(100);
/// How long `Drop` waits for the recorder thread.
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

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

/// Waiting for a save, which blocks the thread: for tests and tools, never for
/// the interface.
pub trait SaveHandleExt {
    /// Blocks until the save finishes, or `None` after `timeout`.
    fn wait(&self, timeout: Duration) -> Option<Result<SavedClip, String>>;
}

impl SaveHandleExt for SaveHandle {
    fn wait(&self, timeout: Duration) -> Option<Result<SavedClip, String>> {
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
    /// Dropped after `Drop::drop` has detached the tap: stops the thread.
    _worker: Worker,
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

        let mut worker = Worker::new(STOP_TIMEOUT);
        let (thread_tap, thread_audio, thread_mic, thread_shared) = (
            Arc::clone(&tap),
            Arc::clone(&audio_tap),
            Arc::clone(&mic_tap),
            Arc::clone(&shared),
        );
        let started = worker.spawn("replay-recorder", move |stop| {
            run(
                &thread_tap,
                &thread_audio,
                &thread_mic,
                &thread_shared,
                stop,
                settings,
            );
        });
        if started {
            slot.set_tap(Some(Arc::clone(&tap)));
        } else {
            shared.set_status(RecorderStatus::Failed(
                "could not start the recorder".into(),
            ));
        }
        Self {
            slot,
            tap,
            audio_tap,
            mic_tap,
            shared,
            _worker: worker,
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
        let handle = SaveHandle::new(Arc::clone(&result));

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
        // No more frames for the thread; then the worker (dropped right after)
        // raises the stop flag and waits for it.
        self.slot.set_tap(None);
    }
}
