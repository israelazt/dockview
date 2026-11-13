//! The service tested with fakes: no capture card, no GPU, no disk, no hotkeys.

use std::collections::VecDeque;
use std::sync::Mutex;

use dockview_domain::recording::{EndReason, RecordingOutcome, SavedClip};

use super::*;

/// What the fakes share with the test: the test sets it up and reads it back.
#[derive(Default)]
struct World {
    status: Option<RecorderStatus>,
    info: Option<RecordingInfo>,
    outcomes: VecDeque<RecordingOutcome>,
    started: Vec<PathBuf>,
    start_error: Option<String>,
    stops: u32,
    saves: Vec<(f64, PathBuf)>,
    save_result: Arc<Mutex<Option<Result<SavedClip, String>>>>,
    keep: Vec<f64>,
    mics_opened: Vec<Option<String>>,
    mic_cleared: u32,
    mic_level: f32,
    hotkeys: (bool, bool),
    free_bytes: Option<u64>,
    folder_ok: bool,
    shown: Vec<(Option<PathBuf>, PathBuf)>,
    buffers_started: u32,
}

type Shared = Arc<Mutex<World>>;

fn world() -> Shared {
    Arc::new(Mutex::new(World {
        status: Some(RecorderStatus::Recording),
        info: Some(idle_info()),
        folder_ok: true,
        free_bytes: Some(100 << 30),
        ..World::default()
    }))
}

fn idle_info() -> RecordingInfo {
    RecordingInfo {
        state: RecordingState::Idle,
        seconds: 0.0,
        bytes: 0,
        low_disk: false,
    }
}

struct FakeMachine(Shared);
struct FakeBuffer(Shared);
struct FakeMic(Shared);
struct FakeHotkeys(Shared);

impl ReplayMachine for FakeMachine {
    fn start_buffer(&self, _: Arc<FrameSlot>, _: ReplaySettings) -> Box<dyn ReplayBuffer> {
        self.0.lock().unwrap().buffers_started += 1;
        Box::new(FakeBuffer(Arc::clone(&self.0)))
    }
    fn open_mic(&self, device: Option<String>, _: Arc<AudioTap>) -> Box<dyn MicInput> {
        self.0.lock().unwrap().mics_opened.push(device);
        Box::new(FakeMic(Arc::clone(&self.0)))
    }
    fn register_hotkeys(&self) -> Box<dyn ReplayHotkeys> {
        Box::new(FakeHotkeys(Arc::clone(&self.0)))
    }
    fn disk_free_bytes(&self, _: &Path) -> Option<u64> {
        self.0.lock().unwrap().free_bytes
    }
    fn new_clip_path(&self, folder: &Path) -> PathBuf {
        folder.join("clip.mp4")
    }
    fn new_recording_path(&self, folder: &Path) -> PathBuf {
        folder.join("recording.mp4")
    }
    fn create_folder(&self, _: &Path) -> bool {
        self.0.lock().unwrap().folder_ok
    }
    fn default_folder(&self) -> PathBuf {
        PathBuf::from("default-clips")
    }
    fn is_writable_folder(&self, folder: &Path) -> bool {
        folder != Path::new("read-only")
    }
    fn hotkey_labels(&self) -> (String, String) {
        ("Ctrl+S".into(), "Ctrl+R".into())
    }
    fn show_saved(&self, newest: Option<&Path>, folder: &Path) {
        self.0
            .lock()
            .unwrap()
            .shown
            .push((newest.map(Path::to_owned), folder.to_owned()));
    }
}

impl ReplayBuffer for FakeBuffer {
    fn status(&self) -> RecorderStatus {
        self.0.lock().unwrap().status.clone().unwrap()
    }
    fn buffered_seconds(&self) -> f64 {
        12.5
    }
    fn buffered_bytes(&self) -> usize {
        3_000_000
    }
    fn set_keep_seconds(&self, seconds: f64) {
        self.0.lock().unwrap().keep.push(seconds);
    }
    fn save(&self, seconds: f64, path: PathBuf) -> SaveHandle {
        let mut w = self.0.lock().unwrap();
        w.saves.push((seconds, path));
        SaveHandle::new(Arc::clone(&w.save_result))
    }
    fn recording(&self) -> RecordingInfo {
        self.0.lock().unwrap().info.unwrap()
    }
    fn start_recording(&self, path: PathBuf) -> Result<(), String> {
        let mut w = self.0.lock().unwrap();
        if let Some(e) = w.start_error.clone() {
            return Err(e);
        }
        w.started.push(path);
        Ok(())
    }
    fn stop_recording(&self) {
        self.0.lock().unwrap().stops += 1;
    }
    fn take_recording_result(&self) -> Option<RecordingOutcome> {
        self.0.lock().unwrap().outcomes.pop_front()
    }
    fn audio_tap(&self) -> Arc<AudioTap> {
        Arc::new(AudioTap::new(4))
    }
    fn mic_tap(&self) -> Arc<AudioTap> {
        Arc::new(AudioTap::new(4))
    }
    fn clear_mic(&self) {
        self.0.lock().unwrap().mic_cleared += 1;
    }
}

impl MicInput for FakeMic {
    fn status(&self) -> AudioStatus {
        AudioStatus::Running
    }
    fn level(&self) -> f32 {
        self.0.lock().unwrap().mic_level
    }
}

impl ReplayHotkeys for FakeHotkeys {
    fn take_save_pressed(&self) -> bool {
        std::mem::take(&mut self.0.lock().unwrap().hotkeys.0)
    }
    fn take_record_pressed(&self) -> bool {
        std::mem::take(&mut self.0.lock().unwrap().hotkeys.1)
    }
}

fn service() -> (ReplayService, Shared) {
    let w = world();
    (ReplayService::new(Box::new(FakeMachine(Arc::clone(&w)))), w)
}

/// A service whose buffer is already running.
fn running() -> (ReplayService, Shared) {
    let (mut s, w) = service();
    s.start_buffer(Arc::new(FrameSlot::new()), ReplaySettings::default());
    (s, w)
}

fn clip(path: &str) -> SavedClip {
    SavedClip {
        path: PathBuf::from(path),
        frames: 10,
        seconds: 1.0,
        bytes: 100,
    }
}

fn folder() -> &'static Path {
    Path::new("clips")
}

// -- pure pieces -------------------------------------------------------

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

// -- the service, with fakes ----------------------------------------------

#[test]
fn a_service_with_nothing_running_is_idle() {
    let (mut s, _) = service();
    assert!(!s.is_on() && !s.mic_on() && !s.recording_active() && !s.saving());
    assert!(s.buffer_info().is_none() && s.recording_info().is_none());
    assert_eq!(s.take_hotkeys(), (false, false));
    assert!(s.take_events().is_empty());
}

#[test]
fn the_buffer_reports_what_it_holds() {
    let (s, _) = running();
    let info = s.buffer_info().expect("on");
    assert_eq!(info.status, RecorderStatus::Recording);
    assert_eq!(info.buffered_seconds, 12.5);
    assert_eq!(info.buffered_bytes, 3_000_000);
    s.set_keep_seconds(32.0);
}

#[test]
fn recording_without_a_buffer_asks_the_app_to_turn_it_on_then_waits() {
    let (mut s, _) = service();
    assert_eq!(s.toggle_record(), RecordToggle::NeedsBuffer);
    assert!(
        !s.waiting_to_record(),
        "the app switches the buffer on first"
    );
    s.wait_for_buffer();
    assert!(s.waiting_to_record());
    // A second press cancels the wait.
    assert_eq!(s.toggle_record(), RecordToggle::WaitCancelled);
    assert!(!s.waiting_to_record());
}

#[test]
fn a_recording_waits_for_the_buffer_then_opens_its_file() {
    let (mut s, w) = running();
    w.lock().unwrap().status = Some(RecorderStatus::Starting);
    assert_eq!(s.toggle_record(), RecordToggle::Waiting);
    s.poll(folder());
    assert!(w.lock().unwrap().started.is_empty(), "still starting");
    assert!(s.waiting_to_record());

    w.lock().unwrap().status = Some(RecorderStatus::Recording);
    s.poll(folder());
    assert_eq!(
        w.lock().unwrap().started,
        vec![folder().join("recording.mp4")]
    );
    assert!(!s.waiting_to_record());
    assert_eq!(s.take_events(), vec![AppEvent::RecordingStarted]);
}

#[test]
fn a_recording_is_refused_when_the_buffer_cannot_record_or_the_disk_is_full() {
    let (mut s, w) = running();
    w.lock().unwrap().status = Some(RecorderStatus::Failed("no encoder".into()));
    s.toggle_record();
    s.poll(folder());
    assert_eq!(s.take_events(), vec![AppEvent::RecordingFailed]);
    assert!(!s.waiting_to_record());

    w.lock().unwrap().status = Some(RecorderStatus::Recording);
    w.lock().unwrap().free_bytes = Some(1 << 30); // less than the 2 GB needed
    s.toggle_record();
    s.poll(folder());
    assert_eq!(s.take_events(), vec![AppEvent::RecordingNoSpace]);
    assert!(w.lock().unwrap().started.is_empty());
}

#[test]
fn a_start_the_recorder_refuses_is_reported() {
    let (mut s, w) = running();
    w.lock().unwrap().start_error = Some("disk".into());
    s.toggle_record();
    s.poll(folder());
    assert_eq!(s.take_events(), vec![AppEvent::RecordingFailed]);
}

#[test]
fn pressing_record_while_recording_stops_it() {
    let (mut s, w) = running();
    w.lock().unwrap().info = Some(RecordingInfo {
        state: RecordingState::Recording,
        ..idle_info()
    });
    assert!(s.recording_active());
    assert_eq!(s.toggle_record(), RecordToggle::Stopped);
    assert_eq!(w.lock().unwrap().stops, 1);
}

#[test]
fn how_a_recording_ended_is_reported_and_its_file_remembered() {
    let (mut s, w) = running();
    w.lock().unwrap().outcomes.push_back(RecordingOutcome {
        result: Ok(clip("clips/a.mp4")),
        reason: EndReason::Stopped,
        continues: false,
    });
    w.lock().unwrap().outcomes.push_back(RecordingOutcome {
        result: Err("disk".into()),
        reason: EndReason::Failed,
        continues: false,
    });
    s.poll(folder());
    assert_eq!(
        s.take_events(),
        vec![AppEvent::RecordingSaved, AppEvent::RecordingFailed]
    );
    s.show_saved(folder());
    assert_eq!(
        w.lock().unwrap().shown,
        vec![(Some(PathBuf::from("clips/a.mp4")), folder().to_owned())]
    );
}

#[test]
fn the_low_disk_warning_is_given_once_while_recording() {
    let (mut s, w) = running();
    w.lock().unwrap().info = Some(RecordingInfo {
        state: RecordingState::Recording,
        low_disk: true,
        ..idle_info()
    });
    s.poll(folder());
    s.poll(folder());
    assert_eq!(s.take_events(), vec![AppEvent::LowDisk]);
}

#[test]
fn a_clip_needs_the_buffer_and_a_folder() {
    let (mut s, w) = service();
    s.save_clip(folder(), 30.0);
    assert_eq!(s.take_events(), vec![AppEvent::ReplayIsOff]);

    let (mut s, w2) = running();
    w2.lock().unwrap().folder_ok = false;
    s.save_clip(folder(), 30.0);
    assert_eq!(s.take_events(), vec![AppEvent::ClipFailed]);
    assert!(w2.lock().unwrap().saves.is_empty() && w.lock().unwrap().saves.is_empty());
}

#[test]
fn a_clip_is_saved_in_the_background_and_reported_when_done() {
    let (mut s, w) = running();
    s.save_clip(folder(), 30.0);
    assert_eq!(s.take_events(), vec![AppEvent::ClipSaving]);
    assert!(s.saving());
    assert_eq!(
        w.lock().unwrap().saves,
        vec![(30.0, folder().join("clip.mp4"))]
    );

    // A second request while one is running does nothing.
    s.save_clip(folder(), 30.0);
    assert_eq!(w.lock().unwrap().saves.len(), 1);
    assert!(s.take_events().is_empty());

    s.poll(folder());
    assert!(s.saving(), "not finished yet");
    *w.lock().unwrap().save_result.lock().unwrap() = Some(Ok(clip("clips/clip.mp4")));
    s.poll(folder());
    assert!(!s.saving());
    assert_eq!(s.take_events(), vec![AppEvent::ClipSaved]);
}

#[test]
fn a_clip_that_could_not_be_written_is_reported() {
    let (mut s, w) = running();
    s.save_clip(folder(), 30.0);
    s.take_events();
    *w.lock().unwrap().save_result.lock().unwrap() = Some(Err("disk".into()));
    s.poll(folder());
    assert_eq!(s.take_events(), vec![AppEvent::ClipFailed]);
}

#[test]
fn the_microphone_needs_the_buffer_and_goes_with_it() {
    let (mut s, w) = service();
    assert!(!s.open_mic(None), "no buffer, no microphone");

    let (mut s, w2) = running();
    assert!(s.open_mic(Some("usb".into())));
    assert!(s.mic_on());
    assert_eq!(s.mic_status(), Some(AudioStatus::Running));
    assert_eq!(w2.lock().unwrap().mics_opened, vec![Some("usb".to_owned())]);

    s.close_mic();
    assert!(!s.mic_on());
    assert_eq!(w2.lock().unwrap().mic_cleared, 1);

    s.open_mic(None);
    s.stop_buffer();
    assert!(
        !s.mic_on() && !s.is_on(),
        "the microphone rides on the buffer"
    );
    assert!(w.lock().unwrap().mics_opened.is_empty());
}

#[test]
fn the_meter_follows_the_microphone() {
    let (mut s, w) = running();
    s.open_mic(None);
    w.lock().unwrap().mic_level = 0.6;
    s.poll(folder());
    assert!((s.mic_level() - 0.6).abs() < 1e-6);
    s.close_mic();
    assert_eq!(s.mic_level(), 0.0);
}

#[test]
fn the_hotkeys_are_read_once() {
    let (mut s, w) = service();
    s.register_hotkeys();
    w.lock().unwrap().hotkeys = (true, false);
    assert_eq!(s.take_hotkeys(), (true, false));
    assert_eq!(s.take_hotkeys(), (false, false), "taken");
}

#[test]
fn starting_the_buffer_closes_a_microphone_left_over() {
    let (mut s, w) = running();
    s.open_mic(None);
    s.start_buffer(Arc::new(FrameSlot::new()), ReplaySettings::default());
    assert!(!s.mic_on());
    assert_eq!(w.lock().unwrap().buffers_started, 2);
}
