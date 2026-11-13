//! Instant replay, clips, recording and the microphone: the state and the
//! mechanics, without windows or settings.
//!
//! The service owns the replay recorder, the clip being saved, the global
//! hotkeys, the microphone and the "record as soon as the buffer is ready"
//! wait. It is given what it needs (a folder, a length, a device) and reports
//! back with [`Notice`]s instead of showing anything itself; the app decides how
//! a notice is shown (today, a toast).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use dockview_core::audio::AudioStatus;
use dockview_core::frame::FrameSlot;
use dockview_core::pcm::AudioTap;
use dockview_core::recorder::{RecorderStatus, ReplayRecorder, ReplaySettings, SaveHandle};
use dockview_core::recording::{disk_free_bytes, EndReason, RecordingState, DISK_START_MIN_BYTES};
use dockview_core::tap_capture::{CaptureSource, TapCapture as MicCapture};

use crate::i18n::Key;
use crate::ui::controls::{MicState, MicView, RecState, RecView, ReplayState, ReplayView};
use crate::ui::toast::ToastKind;
use crate::{clips, hotkey};

/// Something the user should be told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notice {
    pub key: Key,
    pub kind: ToastKind,
}

impl Notice {
    const fn new(key: Key, kind: ToastKind) -> Self {
        Self { key, kind }
    }
}

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

/// The message to show when a recording (or one of its files) ends, if any.
pub fn outcome_notice(reason: EndReason, saved: bool) -> Option<Notice> {
    match (reason, saved) {
        // The user cancelled before anything was recorded: nothing to say.
        (EndReason::Cancelled, _) => None,
        (EndReason::ResolutionChanged, true) => {
            Some(Notice::new(Key::ToastRecNewPart, ToastKind::Info))
        }
        (EndReason::DiskFull, true) => Some(Notice::new(Key::ToastRecDiskFull, ToastKind::Error)),
        (EndReason::NoSignal, _) => Some(Notice::new(Key::ToastRecNoSignal, ToastKind::Error)),
        (_, true) => Some(Notice::new(Key::ToastRecSaved, ToastKind::Success)),
        (_, false) => Some(Notice::new(Key::ToastRecFailed, ToastKind::Error)),
    }
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
    /// The instant-replay buffer; `Some` while it is on.
    recorder: Option<ReplayRecorder>,
    /// A clip being written in the background.
    saving: Option<SaveHandle>,
    hotkey: Option<hotkey::Hotkey>,
    /// The microphone; `Some` only while the user has it on.
    mic: Option<MicCapture>,
    /// Recording was asked for while the buffer was still starting.
    record_when_ready: bool,
    /// The newest clip or recording saved this session, to show in Explorer.
    last_saved: Option<PathBuf>,
    low_disk: LowDiskWarning,
    /// Smoothed loudness for the meter.
    mic_level: f32,
    mic_level_at: Instant,
    notices: Vec<Notice>,
}

impl ReplayService {
    pub fn new() -> Self {
        Self {
            recorder: None,
            saving: None,
            hotkey: None,
            mic: None,
            record_when_ready: false,
            last_saved: None,
            low_disk: LowDiskWarning::default(),
            mic_level: 0.0,
            mic_level_at: Instant::now(),
            notices: Vec::new(),
        }
    }

    /// What happened since the last call, for the app to show.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    fn notify(&mut self, key: Key, kind: ToastKind) {
        self.notices.push(Notice::new(key, kind));
    }

    // -- hotkeys --------------------------------------------------------

    /// Registers the global hotkeys, saying so when another program has them.
    pub fn register_hotkeys(&mut self) {
        let hotkey = hotkey::Hotkey::register();
        if !hotkey.is_registered() {
            eprintln!(
                "replay hotkey {} is taken by another program",
                hotkey::LABEL
            );
        }
        if !hotkey.record_registered() {
            eprintln!(
                "record hotkey {} is taken by another program",
                hotkey::RECORD_LABEL
            );
        }
        self.hotkey = Some(hotkey);
    }

    /// Which hotkeys were pressed since the last call: (save clip, record).
    pub fn take_hotkeys(&self) -> (bool, bool) {
        self.hotkey.as_ref().map_or((false, false), |h| {
            (h.take_pressed(), h.take_record_pressed())
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
        self.recorder = Some(ReplayRecorder::start(slot, settings));
    }

    /// Stops the buffer (and the microphone that rides on it).
    pub fn stop_buffer(&mut self) {
        self.mic = None;
        self.recorder = None;
    }

    /// Where the game's sound goes to be recorded.
    pub fn audio_tap(&self) -> Option<Arc<AudioTap>> {
        self.recorder.as_ref().map(ReplayRecorder::audio_tap)
    }

    pub fn set_keep_seconds(&self, seconds: f64) {
        if let Some(recorder) = &self.recorder {
            recorder.set_keep_seconds(seconds);
        }
    }

    // -- recording ------------------------------------------------------

    /// Whether a recording is running (or starting).
    pub fn recording_active(&self) -> bool {
        self.recorder
            .as_ref()
            .is_some_and(|r| r.recording().state != RecordingState::Idle)
    }

    pub fn rec_view(&self) -> RecView {
        if self.record_when_ready {
            return RecView {
                state: RecState::Starting,
                seconds: 0.0,
            };
        }
        let Some(recorder) = &self.recorder else {
            return RecView::idle();
        };
        let info = recorder.recording();
        RecView {
            state: match info.state {
                RecordingState::Idle => RecState::Idle,
                RecordingState::Starting => RecState::Starting,
                RecordingState::Recording => RecState::Recording,
            },
            seconds: info.seconds as f32,
        }
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
                self.notify(Key::ToastRecFailed, ToastKind::Error);
                return;
            }
        }
        self.record_when_ready = false;
        if disk_free_bytes(folder).is_some_and(|free| free < DISK_START_MIN_BYTES) {
            self.notify(Key::ToastRecNoSpace, ToastKind::Error);
            return;
        }
        let name = clips::recording_file_name(clips::local_time_now());
        let path = clips::unique_path(folder, &name);
        match recorder.start_recording(path) {
            Ok(()) => self.notify(Key::ToastRecStarted, ToastKind::Info),
            Err(e) => {
                eprintln!("recording not started: {e}");
                self.notify(Key::ToastRecFailed, ToastKind::Error);
            }
        }
    }

    // -- clips ----------------------------------------------------------

    /// Saves the last `seconds` to a new file in `folder`, in the background.
    pub fn save_clip(&mut self, folder: &Path, seconds: f64) {
        if self.saving.is_some() {
            return;
        }
        let Some(recorder) = &self.recorder else {
            self.notify(Key::ToastTurnOnFirst, ToastKind::Info);
            return;
        };
        if std::fs::create_dir_all(folder).is_err() {
            self.notify(Key::ToastClipFailed, ToastKind::Error);
            return;
        }
        let name = clips::clip_file_name(clips::local_time_now());
        let path = clips::unique_path(folder, &name);
        self.saving = Some(recorder.save(seconds, path));
        self.notify(Key::ToastSaving, ToastKind::Info);
    }

    /// Opens Explorer on the newest file saved this session, or on `folder`
    /// when there is none (or it was moved).
    pub fn show_saved(&self, folder: &Path) {
        match &self.last_saved {
            Some(file) if file.exists() => clips::reveal_file(file),
            _ => clips::open_folder(folder),
        }
    }

    /// What the replay popover shows about the buffer and the clip in progress.
    pub fn fill_view(&self, view: &mut ReplayView) {
        view.saving = self.saving.is_some();
        if let Some(recorder) = &self.recorder {
            view.state = match recorder.status() {
                RecorderStatus::Starting => ReplayState::Starting,
                RecorderStatus::Recording => ReplayState::Recording,
                RecorderStatus::Unsupported(_) | RecorderStatus::Failed(_) => {
                    ReplayState::Unavailable
                }
            };
            view.buffered_seconds = recorder.buffered_seconds() as f32;
            view.buffered_mb = recorder.buffered_bytes() as f32 / 1e6;
        }
    }

    // -- microphone -----------------------------------------------------

    pub fn mic_on(&self) -> bool {
        self.mic.is_some()
    }

    pub fn mic_view(&self) -> MicView {
        let state = match self.mic.as_ref().map(MicCapture::status) {
            None | Some(AudioStatus::Stopped) => MicState::Off,
            Some(AudioStatus::Starting) => MicState::Starting,
            Some(AudioStatus::Running) => MicState::Live,
            Some(AudioStatus::Disconnected(_) | AudioStatus::Failed(_)) => MicState::Unavailable,
        };
        MicView {
            state,
            level: self.mic_level,
        }
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
        self.mic = Some(MicCapture::start(
            CaptureSource::Device(device),
            1.0,
            recorder.mic_tap(),
        ));
        true
    }

    // -- every tick -----------------------------------------------------

    /// Per-tick work: the meter, a recording waiting to start, finished
    /// recordings and clips, and the low-disk warning.
    pub fn poll(&mut self, folder: &Path) {
        let now = Instant::now();
        let dt = now.duration_since(self.mic_level_at).as_secs_f32();
        self.mic_level_at = now;
        let raw = self.mic.as_ref().map_or(0.0, MicCapture::level);
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
            if let Some(notice) = outcome_notice(outcome.reason, outcome.result.is_ok()) {
                self.notices.push(notice);
            }
        }

        if let Some(info) = self.recorder.as_ref().map(|r| r.recording()) {
            let idle = info.state == RecordingState::Idle;
            if self.low_disk.should_warn(idle, info.low_disk) {
                self.notify(Key::ToastLowDisk, ToastKind::Error);
            }
        }

        if let Some(result) = self.saving.as_ref().and_then(SaveHandle::poll) {
            self.saving = None;
            match result {
                Ok(clip) => {
                    self.last_saved = Some(clip.path);
                    self.notify(Key::ToastClipSaved, ToastKind::Success);
                }
                Err(e) => {
                    eprintln!("clip not saved: {e}");
                    self.notify(Key::ToastClipFailed, ToastKind::Error);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_way_a_recording_can_end_says_the_right_thing() {
        let notice = |key, kind| Some(Notice { key, kind });
        assert_eq!(outcome_notice(EndReason::Cancelled, false), None);
        assert_eq!(
            outcome_notice(EndReason::Stopped, true),
            notice(Key::ToastRecSaved, ToastKind::Success)
        );
        assert_eq!(
            outcome_notice(EndReason::ResolutionChanged, true),
            notice(Key::ToastRecNewPart, ToastKind::Info)
        );
        assert_eq!(
            outcome_notice(EndReason::DiskFull, true),
            notice(Key::ToastRecDiskFull, ToastKind::Error)
        );
        assert_eq!(
            outcome_notice(EndReason::NoSignal, false),
            notice(Key::ToastRecNoSignal, ToastKind::Error)
        );
        assert_eq!(
            outcome_notice(EndReason::Failed, false),
            notice(Key::ToastRecFailed, ToastKind::Error)
        );
    }

    #[test]
    fn pressing_record_does_the_right_thing_in_every_state() {
        // (recording, waiting, buffer on) -> what happens
        assert_eq!(decide_toggle(true, false, true), RecordToggle::Stopped);
        // A recording wins over a stale wait.
        assert_eq!(decide_toggle(true, true, true), RecordToggle::Stopped);
        assert_eq!(
            decide_toggle(false, true, true),
            RecordToggle::WaitCancelled
        );
        assert_eq!(
            decide_toggle(false, false, false),
            RecordToggle::NeedsBuffer
        );
        assert_eq!(decide_toggle(false, false, true), RecordToggle::Waiting);
    }

    #[test]
    fn a_service_with_nothing_running_is_idle() {
        let mut service = ReplayService::new();
        assert!(!service.is_on());
        assert!(!service.mic_on());
        assert!(!service.recording_active());
        assert_eq!(service.rec_view().state, RecState::Idle);
        assert_eq!(service.mic_view().state, MicState::Off);
        assert_eq!(service.take_hotkeys(), (false, false));
        assert!(service.take_notices().is_empty());
    }

    #[test]
    fn recording_without_a_buffer_asks_the_app_to_turn_it_on_then_waits() {
        let mut service = ReplayService::new();
        assert_eq!(service.toggle_record(), RecordToggle::NeedsBuffer);
        // Nothing waits yet: the app switches the buffer on first.
        assert_eq!(service.rec_view().state, RecState::Idle);
        service.wait_for_buffer();
        assert_eq!(service.rec_view().state, RecState::Starting);
        // A second press cancels the wait.
        assert_eq!(service.toggle_record(), RecordToggle::WaitCancelled);
        assert_eq!(service.rec_view().state, RecState::Idle);
    }

    #[test]
    fn saving_a_clip_without_the_buffer_says_to_turn_it_on() {
        let mut service = ReplayService::new();
        service.save_clip(Path::new("."), 30.0);
        assert_eq!(
            service.take_notices(),
            vec![Notice::new(Key::ToastTurnOnFirst, ToastKind::Info)]
        );
        // Taking them empties the list.
        assert!(service.take_notices().is_empty());
    }

    #[test]
    fn the_microphone_cannot_open_without_the_buffer() {
        let mut service = ReplayService::new();
        assert!(!service.open_mic(None));
        assert!(!service.mic_on());
    }

    #[test]
    fn the_low_disk_warning_comes_once_per_recording() {
        let mut warning = LowDiskWarning::default();
        assert!(!warning.should_warn(false, false));
        assert!(warning.should_warn(false, true));
        assert!(!warning.should_warn(false, true), "only once");
        // The recording ends: it rearms for the next one.
        assert!(!warning.should_warn(true, true));
        assert!(warning.should_warn(false, true));
    }

    #[test]
    fn the_meter_rises_at_once_and_falls_smoothly() {
        assert_eq!(smooth_level(0.1, 0.8, 0.016), 0.8);
        let fallen = smooth_level(0.8, 0.0, 0.1);
        assert!((fallen - 0.65).abs() < 1e-5, "{fallen}");
        // A long pause does not drop it more than one capped step.
        assert!((smooth_level(1.0, 0.0, 5.0) - 0.7).abs() < 1e-5);
        assert_eq!(smooth_level(0.0, 0.0, 0.1), 0.0);
        assert_eq!(smooth_level(0.2, 3.0, 0.1), 1.0, "clamped");
    }
}
