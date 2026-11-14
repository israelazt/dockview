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

// -- Renaming a scene ----------------------------------------------------

fn rig_with_scenes(names: &[&str]) -> Rig {
    use dockview_domain::scene::{Scene, SourceRef};
    let scenes = names
        .iter()
        .map(|name| {
            Scene::with_base(
                *name,
                SourceRef::Window {
                    title: "Game".into(),
                },
            )
        })
        .collect();
    Rig::builder()
        .settings(AppSettings {
            scenes,
            ..AppSettings::default()
        })
        .build()
}

#[test]
fn renaming_a_scene_keeps_the_clean_name_and_saves_it() {
    let mut rig = rig_with_scenes(&["One", "Two"]);
    rig.core.apply(Action::RenameScene(1, "  Stream  ".into()));
    let saved = rig.saved_soon().expect("saved");
    assert_eq!(saved.scenes[0].name, "One");
    assert_eq!(saved.scenes[1].name, "Stream");
}

#[test]
fn an_unusable_rename_changes_nothing() {
    let mut rig = rig_with_scenes(&["One"]);
    rig.core.apply(Action::RenameScene(0, "   ".into()));
    rig.core.apply(Action::RenameScene(5, "Ghost".into()));
    assert!(
        rig.saved_soon().is_none(),
        "nothing changed, nothing written"
    );
    assert_eq!(rig.core.model().scenes.names, ["One"]);
}

// -- Editing the layers of a scene -----------------------------------------

use dockview_domain::scene::{NormRect, Scene, SourceRef};
use dockview_domain::view::SceneAction;

/// A rig whose active scene has a base and two layers over it.
fn rig_editing() -> Rig {
    let mut scene = Scene::with_base(
        "Edit",
        SourceRef::Window {
            title: "Base".into(),
        },
    );
    scene.add_layer(SourceRef::Window {
        title: "One".into(),
    });
    scene.add_layer(SourceRef::Window {
        title: "Two".into(),
    });
    Rig::builder()
        .settings(AppSettings {
            scenes: vec![scene],
            active_scene: Some(0),
            ..AppSettings::default()
        })
        .build()
}

fn layers_of(rig: &mut Rig) -> Vec<NormRect> {
    rig.saved_soon().expect("saved").scenes[0]
        .layers
        .iter()
        .map(|l| l.rect)
        .collect()
}

#[test]
fn a_dragged_layer_is_placed_saved_and_kept_inside_the_picture() {
    let mut rig = rig_editing();
    rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
        1,
        [1000, 2000, 3000, 2500],
    )));
    let rects = layers_of(&mut rig);
    assert_eq!(rects[1], NormRect::new(0.1, 0.2, 0.3, 0.25));
    // Nonsense is pulled back inside.
    rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
        2,
        [-9000, 99000, 500, 90000],
    )));
    let rects = layers_of(&mut rig);
    assert!(rects[2].x >= 0.0 && rects[2].y + rects[2].h <= 1.0 + 1e-6);
    // The base can be placed too; a layer that does not exist is ignored.
    rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
        0,
        [0, 0, 5000, 5000],
    )));
    rig.core.apply(Action::Scene(SceneAction::SetLayerRect(
        9,
        [0, 0, 5000, 5000],
    )));
    assert_eq!(layers_of(&mut rig)[0], NormRect::new(0.0, 0.0, 0.5, 0.5));
}

#[test]
fn layers_move_up_and_down_but_the_base_stays_at_the_bottom() {
    let mut rig = rig_editing();
    let names = |rig: &Rig| {
        rig.core
            .model()
            .scenes
            .layers
            .iter()
            .map(|l| l.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&rig), ["Base", "One", "Two"]);
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(1))));
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(1, 1)));
    assert_eq!(names(&rig), ["Base", "Two", "One"]);
    assert_eq!(
        rig.core.model().scenes.selected,
        Some(2),
        "the selection follows"
    );
    // The top one cannot go higher, nor the base anywhere, nor under the base.
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(2, 1)));
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(0, 1)));
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(1, -1)));
    assert_eq!(names(&rig), ["Base", "Two", "One"]);
    rig.core.apply(Action::Scene(SceneAction::MoveLayer(2, -1)));
    assert_eq!(names(&rig), ["Base", "One", "Two"]);
}

#[test]
fn the_editor_is_a_switch_and_the_selection_only_counts_for_existing_layers() {
    let mut rig = rig_editing();
    assert!(!rig.core.model().scenes.editing);
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    assert!(rig.core.model().scenes.editing);
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(2))));
    assert_eq!(rig.core.model().scenes.selected, Some(2));
    rig.core
        .apply(Action::Scene(SceneAction::SelectLayer(Some(7))));
    assert_eq!(
        rig.core.model().scenes.selected,
        None,
        "there is no layer 7"
    );
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    assert!(!rig.core.model().scenes.editing);
}

#[test]
fn escape_closes_the_layer_editor_before_anything_else() {
    use crate::keys::KeyCommand;
    let mut rig = rig_editing();
    rig.core.apply(Action::ToggleStudio);
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    rig.core.on_key(KeyCommand::Escape);
    assert!(!rig.core.model().scenes.editing);
    assert!(
        rig.core.model().studio,
        "the panel stays until the next Esc"
    );
    rig.core.on_key(KeyCommand::Escape);
    assert!(!rig.core.model().studio);
}

#[test]
fn the_editor_is_off_when_no_scene_is_in_use() {
    let mut rig = Rig::new();
    rig.core.apply(Action::Scene(SceneAction::ToggleEdit));
    assert!(!rig.core.model().scenes.editing);
}

// -- Scene hotkeys ---------------------------------------------------------

fn press_scene_hotkey(rig: &mut Rig, slot: usize) {
    rig.replay.lock().unwrap().scene_hotkey = Some(slot);
    rig.advance(std::time::Duration::from_millis(50));
}

#[test]
fn the_scene_hotkeys_pick_a_scene_the_plain_source_or_nothing() {
    let mut rig = rig_with_scenes(&["One", "Two"]);
    assert_eq!(rig.core.model().scenes.active, None);
    press_scene_hotkey(&mut rig, 2);
    assert_eq!(
        rig.core.model().scenes.active,
        Some(1),
        "F2 is the second scene"
    );
    press_scene_hotkey(&mut rig, 1);
    assert_eq!(rig.core.model().scenes.active, Some(0));
    // A slot with no scene behind it does nothing; pressing the same one again
    // keeps it.
    press_scene_hotkey(&mut rig, 7);
    assert_eq!(rig.core.model().scenes.active, Some(0));
    press_scene_hotkey(&mut rig, 1);
    assert_eq!(rig.core.model().scenes.active, Some(0));
    // F12 goes back to the plain source.
    press_scene_hotkey(&mut rig, 0);
    assert_eq!(rig.core.model().scenes.active, None);
}

#[test]
fn a_scene_hotkey_is_a_scene_change_and_gets_saved() {
    let mut rig = rig_with_scenes(&["One", "Two"]);
    press_scene_hotkey(&mut rig, 2);
    assert_eq!(rig.saved_soon().expect("saved").active_scene, Some(1));
}

// -- Studio mode ---------------------------------------------------------

#[test]
fn toggling_the_studio_mode_flips_it_and_saves_it() {
    let mut rig = Rig::new();
    assert!(!rig.core.model().studio);

    rig.core.apply(Action::ToggleStudio);
    assert!(rig.core.model().studio);
    assert!(rig.saved_soon().expect("saved").studio_open);

    rig.core.apply(Action::ToggleStudio);
    assert!(!rig.core.model().studio);
    assert!(!rig.saved_soon().expect("saved").studio_open);
}

#[test]
fn opening_the_studio_opens_it_once_and_leaves_full_screen() {
    let mut rig = Rig::new();
    rig.core.apply(Action::OpenStudio);
    assert!(rig.core.model().studio);
    rig.core.apply(Action::OpenStudio);
    assert!(rig.core.model().studio, "opening twice does not close it");

    rig.core.set_fullscreen(true);
    rig.core.apply(Action::OpenStudio);
    assert!(!rig.core.model().fullscreen, "the panel needs the window");
    assert!(rig.core.model().studio_visible());
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
    assert_eq!(rig.saved_soon().expect("saved").volume, 0.3);
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
    assert!(rig.saved_soon().expect("saved").was_fullscreen);
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
        device_name: r"\\.\DISPLAY1".into(),
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

// -- Saving the settings -------------------------------------------------

fn attempts(rig: &Rig) -> usize {
    rig.save_attempts.load(std::sync::atomic::Ordering::Relaxed)
}

#[test]
fn dragging_a_slider_is_one_or_two_writes_not_one_per_step() {
    let mut rig = Rig::new();
    for step in 1..=60 {
        rig.core.apply(Action::SetVolume(step as f32 / 100.0));
    }
    assert_eq!(
        attempts(&rig),
        0,
        "nothing is written in the middle of a drag"
    );

    rig.advance(Duration::from_millis(600));
    assert_eq!(attempts(&rig), 1);
    assert_eq!(rig.saved().expect("saved").volume, 0.6);

    // More changes right after wait for the pause to pass.
    rig.core.apply(Action::SetVolume(0.9));
    rig.advance(Duration::from_millis(100));
    assert_eq!(attempts(&rig), 1);
    rig.advance(Duration::from_millis(500));
    assert_eq!(attempts(&rig), 2);
    assert_eq!(rig.saved().expect("saved").volume, 0.9);

    // And nothing is written when nothing changed.
    rig.advance(Duration::from_secs(5));
    assert_eq!(attempts(&rig), 2);
}

#[test]
fn closing_the_window_writes_at_once() {
    let mut rig = Rig::new();
    rig.core.apply(Action::SetVolume(0.3));
    assert!(rig.saved().is_none());

    rig.core.save_settings();
    assert_eq!(rig.saved().expect("saved").volume, 0.3);
}

#[test]
fn a_change_is_not_lost_when_the_program_ends_before_the_next_tick() {
    let mut rig = Rig::new();
    rig.core.apply(Action::SetVolume(0.7));
    let store = std::sync::Arc::clone(&rig.store);
    drop(rig);
    assert_eq!(store.last_saved().expect("saved").volume, 0.7);
}

#[test]
fn a_failed_write_is_told_once_and_tried_again_until_it_works() {
    use std::sync::atomic::Ordering;
    let mut rig = Rig::new();
    rig.store_fails.store(true, Ordering::Relaxed);
    rig.core.apply(Action::SetVolume(0.5));

    rig.advance(Duration::from_millis(600));
    assert_eq!(rig.core.take_events(), [AppEvent::SettingsNotSaved]);
    for _ in 0..4 {
        rig.advance(Duration::from_millis(600));
    }
    assert!(attempts(&rig) >= 4, "it keeps trying");
    assert!(rig.core.take_events().is_empty(), "but tells the user once");
    assert!(rig.saved().is_none());

    rig.store_fails.store(false, Ordering::Relaxed);
    rig.advance(Duration::from_millis(600));
    assert_eq!(rig.saved().expect("saved").volume, 0.5);

    // A new failure later is news again.
    rig.store_fails.store(true, Ordering::Relaxed);
    rig.core.apply(Action::SetVolume(0.6));
    rig.advance(Duration::from_millis(600));
    assert_eq!(rig.core.take_events(), [AppEvent::SettingsNotSaved]);
}

// -- The monitor that was being captured ---------------------------------

fn monitor(handle: isize, name: &str) -> MonitorInfo {
    MonitorInfo {
        handle,
        device_name: name.into(),
        width: 1920,
        height: 1080,
        primary: handle == 1,
    }
}

#[test]
fn the_captured_monitor_is_remembered_by_name_and_found_again_in_another_order() {
    use dockview_domain::sources::CaptureTarget;
    let mut rig = Rig::new();
    rig.sources.lock().unwrap().monitors =
        vec![monitor(1, r"\\.\DISPLAY1"), monitor(2, r"\\.\DISPLAY2")];
    rig.core.refresh_sources();
    rig.core.apply(Action::SelectMonitor(1));
    let saved = rig.saved_soon().expect("saved");
    assert_eq!(saved.last_monitor_name.as_deref(), Some(r"\\.\DISPLAY2"));

    // Next run: Windows lists the screens in the other order.
    let next = Rig::builder()
        .settings(saved)
        .monitors(vec![
            monitor(2, r"\\.\DISPLAY2"),
            monitor(1, r"\\.\DISPLAY1"),
        ])
        .build();
    let sources = next.sources.lock().unwrap();
    assert_eq!(
        sources.last_screen().lock().unwrap().target,
        CaptureTarget::Monitor(2)
    );
}

#[test]
fn a_monitor_that_is_no_longer_there_leaves_the_card_on_screen() {
    let saved = AppSettings {
        last_monitor_name: Some(r"\\.\DISPLAY2".into()),
        ..AppSettings::default()
    };
    let rig = Rig::builder()
        .video(&[("UGREEN 25173", "link:ugreen")])
        .audio(&[("HDMI (UGREEN 25173)", "audio:ugreen")])
        .settings(saved)
        .monitors(vec![monitor(1, r"\\.\DISPLAY1")])
        .build();
    assert!(rig.sources.lock().unwrap().screens.is_empty());
    assert_eq!(rig.sources.lock().unwrap().captures.len(), 1);
}

#[test]
fn a_file_from_before_monitor_names_still_restores_by_position() {
    use dockview_domain::sources::CaptureTarget;
    let old = AppSettings {
        last_monitor: Some(1),
        ..AppSettings::default()
    };
    let rig = Rig::builder()
        .settings(old)
        .monitors(vec![
            monitor(1, r"\\.\DISPLAY1"),
            monitor(2, r"\\.\DISPLAY2"),
        ])
        .build();
    assert_eq!(
        rig.sources
            .lock()
            .unwrap()
            .last_screen()
            .lock()
            .unwrap()
            .target,
        CaptureTarget::Monitor(2)
    );
}

// -- The Studio panel by keyboard -----------------------------------------

#[test]
fn tab_opens_and_closes_the_studio_panel_and_remembers_it() {
    use crate::keys::KeyCommand;
    let mut rig = Rig::new();
    rig.core.on_key(KeyCommand::ToggleStudio);
    assert!(rig.core.model().studio);
    assert!(rig.saved_soon().expect("saved").studio_open);

    rig.core.on_key(KeyCommand::ToggleStudio);
    assert!(!rig.core.model().studio);
    assert!(!rig.saved_soon().expect("saved").studio_open);
}

#[test]
fn escape_closes_the_studio_panel_when_nothing_else_is_open() {
    use crate::keys::KeyCommand;
    let mut rig = Rig::builder()
        .settings(AppSettings {
            studio_open: true,
            ..AppSettings::default()
        })
        .build();
    assert!(rig.core.model().studio);

    rig.core.on_key(KeyCommand::Escape);
    assert!(!rig.core.model().studio);
    assert!(!rig.saved_soon().expect("saved").studio_open);

    // With the panel closed, Escape does nothing at all.
    rig.core.on_key(KeyCommand::Escape);
    assert!(!rig.core.model().studio);
    assert!(rig.host.lock().unwrap().fullscreen.is_empty());
}

#[test]
fn escape_in_full_screen_leaves_full_screen_and_keeps_the_panel_for_afterwards() {
    use crate::keys::KeyCommand;
    let mut rig = Rig::builder()
        .settings(AppSettings {
            studio_open: true,
            ..AppSettings::default()
        })
        .build();
    rig.core.on_key(KeyCommand::ToggleFullscreen);
    assert!(
        !rig.core.model().studio_visible(),
        "full screen hides the panel"
    );

    rig.core.on_key(KeyCommand::Escape);
    assert_eq!(rig.host.lock().unwrap().fullscreen, [true, false]);
    assert!(
        rig.core.model().studio_visible(),
        "leaving full screen brings the panel back"
    );
}

#[test]
fn the_panels_close_button_and_the_pills_button_do_the_same_as_tab() {
    let mut rig = Rig::new();
    rig.core.apply(Action::ToggleStudio);
    assert!(rig.core.model().studio);
    rig.core.apply(Action::ToggleStudio);
    assert!(!rig.core.model().studio);
}

#[test]
fn the_command_line_can_start_with_the_studio_panel_open() {
    let rig = Rig::builder()
        .cli(crate::cli::Cli {
            open_studio: true,
            ..crate::cli::Cli::default()
        })
        .build();
    assert!(rig.core.model().studio);
    let closed = Rig::new();
    assert!(!closed.core.model().studio, "closed unless it is asked for");
}
