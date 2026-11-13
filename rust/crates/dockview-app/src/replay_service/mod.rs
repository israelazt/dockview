//! Instant replay, clips, recording and the microphone: the state and the
//! mechanics, without windows, settings or any real hardware.
//!
//! The service owns the replay buffer, the clip being saved, the global
//! hotkeys, the microphone and the "record as soon as the buffer is ready"
//! wait. It works only against the ports in `dockview_domain::ports`, so it is
//! tested with fakes. It is given what it needs (a folder, a length, a device)
//! and reports back with [`AppEvent`]s instead of showing anything itself; the
//! app decides how an event is shown (today, a toast).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use dockview_domain::audio_status::AudioStatus;
use dockview_domain::events::{outcome_event, AppEvent};
use dockview_domain::frame::FrameSlot;
use dockview_domain::pcm::AudioTap;
use dockview_domain::ports::{MicInput, ReplayBuffer, ReplayHotkeys, ReplayMachine};
use dockview_domain::recording::{
    RecorderStatus, RecordingInfo, RecordingState, ReplaySettings, SaveHandle, DISK_START_MIN_BYTES,
};

#[cfg(test)]
mod tests;

/// What pressing "record" means right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordToggle {
    /// A recording was running: it is being stopped.
    Stopped,
    /// It was waiting for the buffer to be ready: that wait is cancelled.
    WaitCancelled,
    /// The replay buffer is off and recording rides on its encoder: the app
    /// must switch the buffer on, then call [`ReplayService::wait_for_buffer`].
    NeedsBuffer,
    /// The buffer is on: recording starts as soon as it is running.
    Waiting,
}

/// Decides what a press of "record" does.
pub fn decide_toggle(recording: bool, waiting: bool, buffer_on: bool) -> RecordToggle {
    if recording {
        RecordToggle::Stopped
    } else if waiting {
        RecordToggle::WaitCancelled
    } else if !buffer_on {
        RecordToggle::NeedsBuffer
    } else {
        RecordToggle::Waiting
    }
}

/// What the buffer reports about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct BufferInfo {
    pub status: RecorderStatus,
    pub buffered_seconds: f64,
    pub buffered_bytes: usize,
}

/// Warns once per recording when the disk runs low.
#[derive(Debug, Default)]
struct LowDiskWarning {
    warned: bool,
}

impl LowDiskWarning {
    /// Whether to warn now. The warning rearms when the recording is over.
    fn should_warn(&mut self, idle: bool, low_disk: bool) -> bool {
        if idle {
            self.warned = false;
            false
        } else if low_disk && !self.warned {
            self.warned = true;
            true
        } else {
            false
        }
    }
}

/// How fast the microphone meter falls, in levels per second.
const METER_FALL_PER_SEC: f32 = 1.5;
/// The longest step the meter is smoothed over, in seconds.
const METER_MAX_STEP: f32 = 0.2;

/// The meter rises at once and falls smoothly.
fn smooth_level(previous: f32, raw: f32, dt_secs: f32) -> f32 {
    let dt = dt_secs.clamp(0.0, METER_MAX_STEP);
    raw.max(previous - dt * METER_FALL_PER_SEC).clamp(0.0, 1.0)
}

pub struct ReplayService {
    machine: Box<dyn ReplayMachine>,
    /// The instant-replay buffer; `Some` while it is on.
    recorder: Option<Box<dyn ReplayBuffer>>,
    /// A clip being written in the background.
    saving: Option<SaveHandle>,
    hotkeys: Option<Box<dyn ReplayHotkeys>>,
    /// The microphone; `Some` only while the user has it on.
    mic: Option<Box<dyn MicInput>>,
    /// Recording was asked for while the buffer was still starting.
    record_when_ready: bool,
    /// The newest clip or recording saved this session, to show in Explorer.
    last_saved: Option<PathBuf>,
    low_disk: LowDiskWarning,
    /// Smoothed loudness for the meter.
    mic_level: f32,
    mic_level_at: Instant,
    events: Vec<AppEvent>,
}

impl ReplayService {
    pub fn new(machine: Box<dyn ReplayMachine>) -> Self {
        Self {
            machine,
            recorder: None,
            saving: None,
            hotkeys: None,
            mic: None,
            record_when_ready: false,
            last_saved: None,
            low_disk: LowDiskWarning::default(),
            mic_level: 0.0,
            mic_level_at: Instant::now(),
            events: Vec::new(),
        }
    }

    /// What happened since the last call, for the app to show.
    pub fn take_events(&mut self) -> Vec<AppEvent> {
        std::mem::take(&mut self.events)
    }

    fn emit(&mut self, event: AppEvent) {
        self.events.push(event);
    }

    // -- hotkeys --------------------------------------------------------

    /// Registers the global hotkeys.
    pub fn register_hotkeys(&mut self) {
        self.hotkeys = Some(self.machine.register_hotkeys());
    }

    /// Which hotkeys were pressed since the last call: (save clip, record).
    pub fn take_hotkeys(&self) -> (bool, bool) {
        self.hotkeys.as_ref().map_or((false, false), |h| {
            (h.take_save_pressed(), h.take_record_pressed())
        })
    }

    // -- the buffer -----------------------------------------------------

    pub fn is_on(&self) -> bool {
        self.recorder.is_some()
    }

    /// Starts the buffer over `slot`. The microphone rides on the recorder, so
    /// it closes first and the app reopens it if it wants it.
    pub fn start_buffer(&mut self, slot: Arc<FrameSlot>, settings: ReplaySettings) {
        self.mic = None;
        self.recorder = Some(self.machine.start_buffer(slot, settings));
    }

    /// Stops the buffer (and the microphone that rides on it).
    pub fn stop_buffer(&mut self) {
        self.mic = None;
        self.recorder = None;
    }

    /// Where the game's sound goes to be recorded.
    pub fn audio_tap(&self) -> Option<Arc<AudioTap>> {
        self.recorder.as_ref().map(|r| r.audio_tap())
    }

    pub fn set_keep_seconds(&self, seconds: f64) {
        if let Some(recorder) = &self.recorder {
            recorder.set_keep_seconds(seconds);
        }
    }

    /// What the buffer reports, or `None` while it is off.
    pub fn buffer_info(&self) -> Option<BufferInfo> {
        self.recorder.as_ref().map(|r| BufferInfo {
            status: r.status(),
            buffered_seconds: r.buffered_seconds(),
            buffered_bytes: r.buffered_bytes(),
        })
    }

    // -- recording ------------------------------------------------------

    /// What the recording is doing, or `None` while the buffer is off.
    pub fn recording_info(&self) -> Option<RecordingInfo> {
        self.recorder.as_ref().map(|r| r.recording())
    }

    /// Whether a recording is running (or starting).
    pub fn recording_active(&self) -> bool {
        self.recording_info()
            .is_some_and(|info| info.state != RecordingState::Idle)
    }

    /// Whether recording was asked for and is waiting for the buffer.
    pub fn waiting_to_record(&self) -> bool {
        self.record_when_ready
    }

    /// Starts or stops recording the session. See [`RecordToggle`].
    pub fn toggle_record(&mut self) -> RecordToggle {
        let decision = decide_toggle(
            self.recording_active(),
            self.record_when_ready,
            self.is_on(),
        );
        match decision {
            RecordToggle::Stopped => {
                if let Some(recorder) = &self.recorder {
                    recorder.stop_recording();
                }
            }
            RecordToggle::WaitCancelled => self.record_when_ready = false,
            RecordToggle::Waiting => self.record_when_ready = true,
            RecordToggle::NeedsBuffer => {}
        }
        decision
    }

    /// Recording starts as soon as the buffer is running.
    pub fn wait_for_buffer(&mut self) {
        self.record_when_ready = true;
    }

    /// Opens the file once the recorder is running.
    fn begin_recording(&mut self, folder: &Path) {
        let Some(recorder) = &self.recorder else {
            self.record_when_ready = false;
            return;
        };
        match recorder.status() {
            RecorderStatus::Recording => {}
            RecorderStatus::Starting => return, // keep waiting
            RecorderStatus::Unsupported(_) | RecorderStatus::Failed(_) => {
                self.record_when_ready = false;
                self.emit(AppEvent::RecordingFailed);
                return;
            }
        }
        self.record_when_ready = false;
        if self
            .machine
            .disk_free_bytes(folder)
            .is_some_and(|free| free < DISK_START_MIN_BYTES)
        {
            self.emit(AppEvent::RecordingNoSpace);
            return;
        }
        let path = self.machine.new_recording_path(folder);
        let started = self.recorder.as_ref().map(|r| r.start_recording(path));
        match started {
            Some(Ok(())) => self.emit(AppEvent::RecordingStarted),
            Some(Err(e)) => {
                eprintln!("recording not started: {e}");
                self.emit(AppEvent::RecordingFailed);
            }
            None => {}
        }
    }

    // -- clips ----------------------------------------------------------

    /// Saves the last `seconds` to a new file in `folder`, in the background.
    pub fn save_clip(&mut self, folder: &Path, seconds: f64) {
        if self.saving.is_some() {
            return;
        }
        if self.recorder.is_none() {
            self.emit(AppEvent::ReplayIsOff);
            return;
        }
        if !self.machine.create_folder(folder) {
            self.emit(AppEvent::ClipFailed);
            return;
        }
        let path = self.machine.new_clip_path(folder);
        self.saving = self.recorder.as_ref().map(|r| r.save(seconds, path));
        self.emit(AppEvent::ClipSaving);
    }

    /// Whether a clip is being written.
    pub fn saving(&self) -> bool {
        self.saving.is_some()
    }

    /// Opens Explorer on the newest file saved this session, or on `folder`
    /// when there is none (or it was moved).
    pub fn show_saved(&self, folder: &Path) {
        self.machine.show_saved(self.last_saved.as_deref(), folder);
    }

    // -- microphone -----------------------------------------------------

    pub fn mic_on(&self) -> bool {
        self.mic.is_some()
    }

    /// The microphone's state, or `None` while it is off.
    pub fn mic_status(&self) -> Option<AudioStatus> {
        self.mic.as_ref().map(|m| m.status())
    }

    /// The smoothed loudness of the microphone, 0.0 to 1.0.
    pub fn mic_level(&self) -> f32 {
        self.mic_level
    }

    pub fn close_mic(&mut self) {
        self.mic = None;
        if let Some(recorder) = &self.recorder {
            recorder.clear_mic();
        }
        self.mic_level = 0.0;
    }

    /// Opens the microphone (`None` is the Windows default input). It needs
    /// the buffer, which carries its audio; returns whether it could open.
    pub fn open_mic(&mut self, device: Option<String>) -> bool {
        let Some(recorder) = &self.recorder else {
            return false;
        };
        self.mic = Some(self.machine.open_mic(device, recorder.mic_tap()));
        true
    }

    // -- every tick -----------------------------------------------------

    /// Per-tick work: the meter, a recording waiting to start, finished
    /// recordings and clips, and the low-disk warning.
    pub fn poll(&mut self, folder: &Path) {
        let now = Instant::now();
        let dt = now.duration_since(self.mic_level_at).as_secs_f32();
        self.mic_level_at = now;
        let raw = self.mic.as_ref().map_or(0.0, |m| m.level());
        self.mic_level = smooth_level(self.mic_level, raw, dt);

        if self.record_when_ready {
            self.begin_recording(folder);
        }

        let mut outcomes = Vec::new();
        if let Some(recorder) = &self.recorder {
            while let Some(outcome) = recorder.take_recording_result() {
                outcomes.push(outcome);
            }
        }
        for outcome in outcomes {
            if let Err(e) = &outcome.result {
                eprintln!("recording ended with an error: {e}");
            }
            if let Ok(clip) = &outcome.result {
                self.last_saved = Some(clip.path.clone());
            }
            if let Some(event) = outcome_event(outcome.reason, outcome.result.is_ok()) {
                self.emit(event);
            }
        }

        if let Some(info) = self.recording_info() {
            let idle = info.state == RecordingState::Idle;
            if self.low_disk.should_warn(idle, info.low_disk) {
                self.emit(AppEvent::LowDisk);
            }
        }

        if let Some(result) = self.saving.as_ref().and_then(SaveHandle::poll) {
            self.saving = None;
            match result {
                Ok(clip) => {
                    self.last_saved = Some(clip.path);
                    self.emit(AppEvent::ClipSaved);
                }
                Err(e) => {
                    eprintln!("clip not saved: {e}");
                    self.emit(AppEvent::ClipFailed);
                }
            }
        }
    }
}
