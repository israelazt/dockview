//! `Core` driven as a whole, on fakes: no capture card, no sound card, no GPU
//! and no window. These tests use only what the binary uses (`apply`, `on_key`,
//! `tick_at`, `model`, `take_events`) and what the fakes recorded.

use std::time::Duration;

use dockview_domain::device::{AudioDevice, DeviceLists, VideoDevice};
use dockview_domain::events::AppEvent;
use dockview_domain::recording::{RecordingInfo, RecordingState};
use dockview_domain::settings::AppSettings;
use dockview_domain::sources::{CaptureStatus, MonitorInfo, ScreenStatus};
use dockview_domain::view::{Action, StatusKind};

use crate::testing::rig::Rig;

const SECOND: Duration = Duration::from_secs(1);

fn settings_with(video: &str, audio: &str) -> AppSettings {
    AppSettings {
        last_video_device_symbolic_link: Some(video.into()),
        last_audio_device_id: Some(audio.into()),
        ..AppSettings::default()
    }
}

// -- Studio mode ---------------------------------------------------------

#[test]
fn toggling_the_studio_mode_flips_it_and_saves_it() {
    let mut rig = Rig::new();
    assert!(!rig.core.model().studio);

    rig.core.apply(Action::ToggleStudio);
    assert!(rig.core.model().studio);
    assert!(rig.saved().expect("saved").studio_open);

    rig.core.apply(Action::ToggleStudio);
    assert!(!rig.core.model().studio);
    assert!(!rig.saved().expect("saved").studio_open);
}

#[test]
fn the_studio_mode_comes_back_when_the_settings_say_so() {
    let rig = Rig::builder()
        .settings(AppSettings {
            studio_open: true,
            ..AppSettings::default()
        })
        .build();
    assert!(rig.core.model().studio);
}

// -- Starting ------------------------------------------------------------

#[test]
fn it_starts_on_the_card_used_last_time_with_its_own_audio() {
    let rig = Rig::builder()
        .video(&[
            ("UGREEN 25173", "link:ugreen"),
            ("Integrated Camera", "link:webcam"),
        ])
        .audio(&[
            ("Microphone (Integrated Camera)", "audio:webcam"),
            ("HDMI (UGREEN 25173)", "audio:ugreen"),
        ])
        .settings(settings_with("link:webcam", "audio:webcam"))
        .build();
    let sources = rig.sources.lock().unwrap();
    assert_eq!(sources.last_capture().lock().unwrap().link, "link:webcam");
    assert_eq!(
        sources.last_playback().lock().unwrap().device_id,
        "audio:webcam"
    );
}

#[test]
fn the_audio_that_belongs_to_the_card_wins_over_a_pair_saved_out_of_step() {
    // The UGREEN with the webcam's microphone: the bug fixed in 31c45f6.
    let rig = Rig::builder()
        .video(&[
            ("UGREEN 25173", "link:ugreen"),
            ("Integrated Camera", "link:webcam"),
        ])
        .audio(&[
            ("Microphone (Integrated Camera)", "audio:webcam"),
            ("HDMI (UGREEN 25173)", "audio:ugreen"),
        ])
        .settings(settings_with("link:ugreen", "audio:webcam"))
        .build();
    let sources = rig.sources.lock().unwrap();
    assert_eq!(sources.last_capture().lock().unwrap().link, "link:ugreen");
    assert_eq!(
        sources.last_playback().lock().unwrap().device_id,
        "audio:ugreen"
    );
}

#[test]
fn with_nothing_plugged_in_nothing_is_opened() {
    let rig = Rig::builder().build();
    let sources = rig.sources.lock().unwrap();
    assert!(sources.captures.is_empty());
    assert!(sources.playbacks.is_empty());
    drop(sources);
    assert_eq!(rig.core.model().status, StatusKind::Idle);
}

// -- Actions -------------------------------------------------------------

#[test]
fn choosing_another_card_closes_the_old_one_and_follows_with_its_audio() {
    let mut rig = Rig::new();
    rig.core.apply(Action::SelectVideo(1));

    let sources = rig.sources.lock().unwrap();
    assert_eq!(sources.captures.len(), 2);
    assert_eq!(sources.open_captures(), 1, "the old card must be released");
    assert_eq!(sources.last_capture().lock().unwrap().link, "link:webcam");
    assert_eq!(sources.open_playbacks(), 1);
    assert_eq!(
        sources.last_playback().lock().unwrap().device_id,
        "audio:webcam"
    );
}

#[test]
fn the_volume_reaches_the_speakers_and_is_saved() {
    let mut rig = Rig::new();
    rig.core.apply(Action::SetVolume(0.3));

    let playback = rig.sources.lock().unwrap().last_playback();
    assert_eq!(playback.lock().unwrap().volume, 0.3);
    assert_eq!(rig.saved().expect("saved").volume, 0.3);
}

#[test]
fn the_sync_offset_reaches_the_speakers() {
    let mut rig = Rig::new();
    rig.core.apply(Action::SetSync(-40));
    let playback = rig.sources.lock().unwrap().last_playback();
    assert_eq!(playback.lock().unwrap().offset_ms, -40);
}

#[test]
fn the_replay_cannot_be_turned_off_while_recording() {
    let mut rig = Rig::new();
    rig.replay.lock().unwrap().info = Some(RecordingInfo {
        state: RecordingState::Recording,
        seconds: 3.0,
        bytes: 1000,
        low_disk: false,
    });
    let state_before = rig.core.model().replay.state;

    rig.core.apply(Action::ToggleReplay);

    assert_eq!(rig.core.model().replay.state, state_before);
    assert_eq!(rig.core.take_events(), [AppEvent::StopRecordingFirst]);
}

// -- Recovery of the picture ---------------------------------------------

#[test]
fn a_card_that_drops_out_is_opened_again_after_a_pause() {
    let mut rig = Rig::new();
    rig.sources
        .lock()
        .unwrap()
        .last_capture()
        .lock()
        .unwrap()
        .status = CaptureStatus::Disconnected {
        reason: "unplugged".into(),
    };

    rig.advance(Duration::from_millis(100)); // starts the pause
    rig.advance(SECOND);
    assert_eq!(rig.sources.lock().unwrap().captures.len(), 1, "too soon");

    rig.advance(SECOND + SECOND / 2);
    let sources = rig.sources.lock().unwrap();
    assert_eq!(sources.captures.len(), 2, "it must try again");
    assert_eq!(sources.open_captures(), 1);
    assert_eq!(sources.last_capture().lock().unwrap().link, "link:ugreen");
}

#[test]
fn a_captured_window_that_closes_sends_the_picture_back_to_the_card() {
    let mut rig = Rig::new();
    rig.sources.lock().unwrap().monitors = vec![MonitorInfo {
        handle: 7,
        device_name: r"\\.\DISPLAY1".into(),
        width: 1920,
        height: 1080,
        primary: true,
    }];
    rig.core.refresh_sources();
    rig.core.apply(Action::SelectMonitor(0));
    assert_eq!(rig.sources.lock().unwrap().screens.len(), 1);

    rig.sources
        .lock()
        .unwrap()
        .last_screen()
        .lock()
        .unwrap()
        .status = ScreenStatus::Ended("closed".into());
    rig.advance(Duration::from_millis(100));

    assert_eq!(rig.core.take_events(), [AppEvent::SourceLost]);
    let sources = rig.sources.lock().unwrap();
    assert_eq!(sources.last_capture().lock().unwrap().link, "link:ugreen");
    assert_eq!(sources.open_captures(), 1);
}

// -- The rest of the machine ---------------------------------------------

#[test]
fn full_screen_is_asked_of_the_window_and_remembered() {
    let mut rig = Rig::new();
    rig.core.on_key(crate::keys::KeyCommand::ToggleFullscreen);

    assert_eq!(rig.host.lock().unwrap().fullscreen, [true]);
    assert!(rig.saved().expect("saved").was_fullscreen);
}

#[test]
fn the_measurements_close_once_a_second_and_reach_the_log() {
    let mut rig = Rig::new();
    rig.system.usage.lock().unwrap().cpu_core_percent = 80.0;

    rig.advance(SECOND + SECOND / 10);

    assert_eq!(rig.core.stats().cpu_core_percent, 80.0);
    assert!(!rig.stats.lock().unwrap().is_empty());
}

#[test]
fn asking_for_the_compressed_mode_reopens_the_card_with_it() {
    use dockview_domain::settings::VideoModePref;
    let mut rig = Rig::new();
    rig.core
        .apply(Action::SetVideoMode(VideoModePref::Compressed));

    let sources = rig.sources.lock().unwrap();
    assert_eq!(sources.captures.len(), 2);
    assert!(
        sources
            .last_capture()
            .lock()
            .unwrap()
            .options
            .prefer_compressed
    );
}

#[test]
fn capturing_a_monitor_listens_to_the_sound_of_the_pc() {
    use dockview_domain::ports::Loopback;
    use dockview_domain::sources::CaptureTarget;
    let mut rig = Rig::new();
    rig.sources.lock().unwrap().monitors = vec![MonitorInfo {
        handle: 7,
        device_name: r"\.\DISPLAY1".into(),
        width: 1920,
        height: 1080,
        primary: true,
    }];
    rig.core.refresh_sources();
    rig.core.apply(Action::SelectMonitor(0));

    let sources = rig.sources.lock().unwrap();
    assert_eq!(
        sources.last_screen().lock().unwrap().target,
        CaptureTarget::Monitor(7)
    );
    assert_eq!(sources.loopbacks.len(), 1);
    assert_eq!(
        sources.loopbacks[0].lock().unwrap().source,
        Loopback::System
    );
    // The card's own sound is not played while a screen is shown.
    assert_eq!(sources.open_playbacks(), 0);
    // And the window is hidden from the capture, so it is not shown inside itself.
    assert_eq!(rig.host.lock().unwrap().exclusions.last(), Some(&true));
}

#[test]
fn the_command_line_can_open_the_stats_panel() {
    let rig = Rig::builder()
        .cli(crate::cli::Cli {
            show_stats: true,
            ..crate::cli::Cli::default()
        })
        .build();
    assert!(rig.core.model().stats_open);
}

// -- Recovery of the sound -----------------------------------------------

#[test]
fn healthy_sound_is_left_alone() {
    let mut rig = Rig::new();
    for _ in 0..10 {
        rig.advance(SECOND);
    }
    assert_eq!(rig.sources.lock().unwrap().playbacks.len(), 1);
    assert!(rig.core.take_events().is_empty());
}

#[test]
fn lost_sound_is_started_again_after_a_pause_and_the_user_is_told() {
    use dockview_domain::audio_status::AudioStatus;
    let mut rig = Rig::new();
    rig.core.apply(Action::SetVolume(0.4));
    rig.core.apply(Action::SetSync(30));
    rig.sources
        .lock()
        .unwrap()
        .last_playback()
        .lock()
        .unwrap()
        .status = AudioStatus::Disconnected("unplugged".into());

    rig.advance(Duration::from_millis(100)); // starts the pause
    assert_eq!(rig.core.take_events(), [AppEvent::AudioLost]);
    rig.advance(SECOND);
    assert_eq!(rig.sources.lock().unwrap().playbacks.len(), 1, "too soon");

    rig.advance(SECOND + SECOND / 2);
    {
        let sources = rig.sources.lock().unwrap();
        assert_eq!(sources.playbacks.len(), 2, "it must try again");
        assert_eq!(sources.open_playbacks(), 1, "the dead one is let go");
        let playback = sources.last_playback();
        let playback = playback.lock().unwrap();
        assert_eq!(playback.device_id, "audio:ugreen");
        assert_eq!((playback.volume, playback.offset_ms), (0.4, 30));
        assert!(playback.tapped, "the recorder must be fed again");
    }

    rig.advance(Duration::from_millis(100));
    assert_eq!(rig.core.take_events(), [AppEvent::AudioBack]);
}

#[test]
fn sound_that_keeps_failing_is_retried_but_announced_once() {
    use dockview_domain::audio_status::AudioStatus;
    let mut rig = Rig::new();
    rig.sources
        .lock()
        .unwrap()
        .last_playback()
        .lock()
        .unwrap()
        .status = AudioStatus::Failed("no device".into());
    rig.advance(Duration::from_millis(100));
    assert_eq!(rig.core.take_events(), [AppEvent::AudioLost]);

    // Every new attempt fails too.
    for _ in 0..3 {
        rig.advance(Duration::from_millis(2100));
        rig.sources
            .lock()
            .unwrap()
            .last_playback()
            .lock()
            .unwrap()
            .status = AudioStatus::Failed("no device".into());
    }
    assert!(rig.sources.lock().unwrap().playbacks.len() >= 3);
    assert!(rig.core.take_events().is_empty());
}

// -- Plugging and unplugging ---------------------------------------------

fn ugreen() -> (VideoDevice, AudioDevice) {
    (
        VideoDevice {
            friendly_name: "UGREEN 25173".into(),
            symbolic_link: "link:ugreen".into(),
        },
        AudioDevice {
            friendly_name: "HDMI (UGREEN 25173)".into(),
            device_id: "audio:ugreen".into(),
        },
    )
}

fn webcam() -> (VideoDevice, AudioDevice) {
    (
        VideoDevice {
            friendly_name: "Integrated Camera".into(),
            symbolic_link: "link:webcam".into(),
        },
        AudioDevice {
            friendly_name: "Microphone (Integrated Camera)".into(),
            device_id: "audio:webcam".into(),
        },
    )
}

fn plug(rig: &Rig, devices: &[(VideoDevice, AudioDevice)]) {
    rig.sources.lock().unwrap().devices = DeviceLists {
        video: devices.iter().map(|d| d.0.clone()).collect(),
        audio: devices.iter().map(|d| d.1.clone()).collect(),
    };
}

#[test]
fn a_card_plugged_in_after_starting_is_opened_with_its_own_audio() {
    let mut rig = Rig::builder().build();
    assert!(rig.sources.lock().unwrap().captures.is_empty());

    plug(&rig, &[webcam(), ugreen()]);
    rig.advance(Duration::from_millis(100));

    let sources = rig.sources.lock().unwrap();
    // Nothing was open, so it takes the first card, like a normal start.
    assert_eq!(sources.last_capture().lock().unwrap().link, "link:webcam");
    assert_eq!(
        sources.last_playback().lock().unwrap().device_id,
        "audio:webcam"
    );
    drop(sources);
    assert_eq!(rig.core.model().video_devices.len(), 2);
}

#[test]
fn an_extra_device_plugged_in_does_not_disturb_what_is_running() {
    let mut rig = Rig::builder()
        .video(&[("UGREEN 25173", "link:ugreen")])
        .audio(&[("HDMI (UGREEN 25173)", "audio:ugreen")])
        .build();

    plug(&rig, &[webcam(), ugreen()]); // the new one lands first in the list
    rig.advance(Duration::from_millis(100));

    let sources = rig.sources.lock().unwrap();
    assert_eq!(sources.captures.len(), 1, "no restart");
    assert_eq!(sources.playbacks.len(), 1, "no restart");
    drop(sources);
    let model = rig.core.model();
    assert_eq!(model.video_devices.len(), 2);
    assert_eq!(
        model.video_selected,
        Some(1),
        "the selection followed its card"
    );
}

#[test]
fn unplugging_the_card_does_not_move_the_picture_to_the_webcam() {
    let mut rig = Rig::new(); // UGREEN selected, webcam also listed
    plug(&rig, &[webcam()]);
    rig.sources
        .lock()
        .unwrap()
        .last_capture()
        .lock()
        .unwrap()
        .status = CaptureStatus::Disconnected {
        reason: "unplugged".into(),
    };

    for _ in 0..6 {
        rig.advance(SECOND);
    }

    let sources = rig.sources.lock().unwrap();
    assert_eq!(
        sources.captures.len(),
        1,
        "no retries on a card that is gone"
    );
    drop(sources);
    let model = rig.core.model();
    assert_eq!(model.video_selected, None);
    assert_eq!(model.status, StatusKind::Problem);
}

#[test]
fn the_card_that_comes_back_is_used_again_with_its_audio() {
    let mut rig = Rig::new();
    plug(&rig, &[webcam()]);
    rig.sources
        .lock()
        .unwrap()
        .last_capture()
        .lock()
        .unwrap()
        .status = CaptureStatus::Disconnected {
        reason: "unplugged".into(),
    };
    rig.advance(SECOND);

    plug(&rig, &[webcam(), ugreen()]);
    rig.advance(SECOND);

    let sources = rig.sources.lock().unwrap();
    assert_eq!(sources.captures.len(), 2);
    assert_eq!(sources.open_captures(), 1);
    assert_eq!(sources.last_capture().lock().unwrap().link, "link:ugreen");
    assert_eq!(
        sources.last_playback().lock().unwrap().device_id,
        "audio:ugreen"
    );
    drop(sources);
    assert_eq!(rig.core.model().video_selected, Some(1));
}

#[test]
fn an_audio_input_that_comes_back_alone_is_used_again() {
    let mut rig = Rig::new();
    // Only the audio of the card goes away (a flaky cable).
    rig.sources
        .lock()
        .unwrap()
        .devices
        .audio
        .retain(|a| a.device_id != "audio:ugreen");
    rig.advance(SECOND);
    assert_eq!(rig.core.model().audio_selected, None);

    plug(&rig, &[webcam(), ugreen()]);
    rig.advance(SECOND);

    let playback = rig.sources.lock().unwrap().last_playback();
    assert_eq!(playback.lock().unwrap().device_id, "audio:ugreen");
    assert!(rig.core.model().audio_selected.is_some());
}
