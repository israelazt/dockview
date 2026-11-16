//! The service tested with fakes: no capture card, no GPU, no disk, no hotkeys.

use dockview_domain::recording::{EndReason, RecordingOutcome, SavedClip};

use super::*;
use crate::testing::replay::{idle_info, world, FakeMachine, Shared};

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
