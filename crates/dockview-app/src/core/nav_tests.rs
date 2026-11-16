//! The navigation row in `Core`, on fakes.

use std::time::Instant;

use dockview_domain::broadcast::StreamState;
use dockview_domain::frame::{ColorInfo, Frame, PixelFormat};
use dockview_domain::navigation::{AppMode, EncoderInfo, EncoderState};
use dockview_domain::recording::RecorderStatus;
use dockview_domain::view::Action;

use crate::testing::rig::Rig;

fn tab(rig: &Rig) -> AppMode {
    rig.core.model().mode
}

#[test]
fn it_starts_on_the_viewer_with_the_panel_closed() {
    let rig = Rig::new();
    assert_eq!(tab(&rig), AppMode::Viewer);
    assert!(!rig.core.model().mode.shows_panel());
}

#[test]
fn the_studio_tab_opens_the_panel_and_the_viewer_tab_closes_it() {
    let mut rig = Rig::new();
    rig.core.apply(Action::GoTo(AppMode::Studio));
    assert!(rig.core.model().mode.shows_panel());
    assert_eq!(tab(&rig), AppMode::Studio);
    rig.core.apply(Action::GoTo(AppMode::Viewer));
    assert!(!rig.core.model().mode.shows_panel());
    assert_eq!(tab(&rig), AppMode::Viewer);
}

#[test]
fn settings_is_a_mode_of_its_own_without_the_studio_panel_and_esc_goes_back() {
    use crate::keys::KeyCommand;
    let mut rig = Rig::new();
    rig.core.apply(Action::GoTo(AppMode::Settings));
    assert_eq!(tab(&rig), AppMode::Settings);
    assert!(!rig.core.model().studio_visible(), "no panel in Settings");
    rig.core.on_key(KeyCommand::Escape);
    assert_eq!(tab(&rig), AppMode::Viewer, "back to where it came from");
    // From the Studio it goes back to the Studio.
    rig.core.apply(Action::GoTo(AppMode::Studio));
    rig.core.apply(Action::GoTo(AppMode::Settings));
    rig.core.on_key(KeyCommand::Escape);
    assert_eq!(tab(&rig), AppMode::Studio);
    assert!(rig.core.model().studio_visible());
}

#[test]
fn tab_opens_the_studio_from_the_viewer_and_from_settings_and_closes_it() {
    let mut rig = Rig::new();
    rig.core.apply(Action::ToggleStudio);
    assert_eq!(tab(&rig), AppMode::Studio);
    rig.core.apply(Action::ToggleStudio);
    assert_eq!(tab(&rig), AppMode::Viewer);
    rig.core.apply(Action::GoTo(AppMode::Settings));
    rig.core.apply(Action::ToggleStudio);
    assert_eq!(
        tab(&rig),
        AppMode::Studio,
        "from Settings it goes to the Studio"
    );
}

#[test]
fn full_screen_hides_the_panel_without_leaving_the_mode() {
    let mut rig = Rig::new();
    rig.core.apply(Action::GoTo(AppMode::Studio));
    rig.core.apply(Action::ToggleFullscreen);
    assert!(
        !rig.core.model().studio_visible(),
        "the panel is hidden in full screen"
    );
    assert_eq!(tab(&rig), AppMode::Studio, "but the mode is still on");
}

#[test]
fn opening_settings_leaves_full_screen_to_show_it() {
    let mut rig = Rig::new();
    rig.core.apply(Action::ToggleFullscreen);
    rig.core.apply(Action::GoTo(AppMode::Settings));
    assert!(!rig.core.model().fullscreen);
    assert_eq!(tab(&rig), AppMode::Settings);
}

fn encoder(rig: &Rig) -> (EncoderState, String) {
    let nav = rig.core.model().nav;
    (nav.encoder, nav.encoder_name)
}

#[test]
fn the_encoder_is_unknown_until_the_machine_says_and_then_ready() {
    let mut rig = Rig::new();
    assert_eq!(encoder(&rig).0, EncoderState::Unknown);
    rig.replay.lock().unwrap().encoder = EncoderInfo::Found("NVIDIA H.264 Encoder MFT".into());
    // The replay buffer runs by default in the rig, so it is in use.
    assert_eq!(encoder(&rig), (EncoderState::Active, "NVENC".to_string()));
    rig.core.apply(Action::ToggleReplay);
    assert_eq!(encoder(&rig), (EncoderState::Ready, "NVENC".to_string()));
}

#[test]
fn no_encoder_or_a_replay_that_cannot_record_says_so() {
    let rig = Rig::new();
    rig.replay.lock().unwrap().encoder = EncoderInfo::Missing;
    assert_eq!(encoder(&rig).0, EncoderState::Unavailable);
    rig.replay.lock().unwrap().encoder = EncoderInfo::Found("Intel Quick Sync".into());
    rig.replay.lock().unwrap().status = Some(RecorderStatus::Failed("boom".into()));
    assert_eq!(encoder(&rig).0, EncoderState::Unavailable);
}

#[test]
fn the_stream_counts_as_using_the_encoder_and_is_reported() {
    let mut rig = Rig::new();
    rig.core.apply(Action::ToggleReplay); // off
    rig.replay.lock().unwrap().encoder = EncoderInfo::Found("AMD".into());
    rig.core
        .apply(Action::SetStreamUrl("rtmp://live.example/app".into()));
    rig.core.apply(Action::SetStreamKey("k".into()));
    assert_eq!(rig.core.model().nav.stream, StreamState::Offline);
    rig.core.apply(Action::ToggleStream);
    rig.stream.lock().unwrap().state = StreamState::Live;
    let nav = rig.core.model().nav;
    assert_eq!(nav.stream, StreamState::Live);
    assert_eq!(
        (nav.encoder, nav.encoder_name.as_str()),
        (EncoderState::Active, "AMF")
    );
}

#[test]
fn pictures_the_viewer_never_showed_are_a_rate_that_falls_back_to_zero() {
    use std::time::Duration;
    let mut rig = Rig::new();
    assert_eq!(rig.core.model().nav.drops_per_sec, 0.0);
    let slot = rig.core.output_slot().clone();
    let picture = || Frame {
        width: 2,
        height: 2,
        stride: 2,
        format: PixelFormat::Nv12,
        color: ColorInfo::default(),
        data: vec![0; 6],
        timestamp: 0,
        arrived: Instant::now(),
    };
    slot.publish(picture());
    slot.publish(picture()); // replaces the first one before it was shown
    rig.advance(Duration::from_millis(1100));
    let rate = rig.core.model().nav.drops_per_sec;
    assert!(rate > 0.5 && rate < 1.2, "about one per second: {rate}");
    // Nothing more is lost in the next second: the figure goes back down
    // instead of staying at the total.
    rig.advance(Duration::from_millis(1100));
    assert_eq!(rig.core.model().nav.drops_per_sec, 0.0);
}

#[test]
#[ignore = "the screenshot tool is off (snap::ENABLED)"]
fn the_capture_button_starts_a_screenshot_in_the_chosen_mode() {
    use dockview_domain::select::Mode;
    let mut rig = Rig::new();
    rig.core.apply(Action::Capture);
    assert_eq!(rig.host.lock().unwrap().captures, [Mode::Rectangle]);
}

#[test]
fn the_readouts_can_be_hidden_and_shown_and_it_is_remembered() {
    let mut rig = Rig::new();
    assert!(rig.core.model().hud);
    rig.core.apply(Action::ToggleHud);
    assert!(!rig.core.model().hud);
    assert!(!rig.saved_soon().unwrap().hud);
    rig.core.apply(Action::ToggleHud);
    assert!(rig.core.model().hud);
}

#[test]
fn the_sync_the_undo_and_the_source_keys_only_work_in_the_studio() {
    use crate::keys::KeyCommand;
    let mut rig = Rig::new();
    let sync = |rig: &Rig| rig.core.model().sync_ms;
    // In the viewer they do nothing.
    rig.core.on_key(KeyCommand::SyncLater);
    assert_eq!(sync(&rig), 0);
    rig.core.apply(Action::GoTo(AppMode::Settings));
    rig.core.on_key(KeyCommand::SyncLater);
    assert_eq!(sync(&rig), 0);
    // In the Studio they work.
    rig.core.apply(Action::GoTo(AppMode::Studio));
    rig.core.on_key(KeyCommand::SyncLater);
    assert!(sync(&rig) > 0);
}

#[test]
fn the_volume_keys_work_in_every_mode() {
    use crate::keys::KeyCommand;
    for mode in [AppMode::Viewer, AppMode::Studio, AppMode::Settings] {
        let mut rig = Rig::new();
        rig.core.apply(Action::GoTo(mode));
        rig.core.on_key(KeyCommand::VolumeDown);
        assert!(rig.core.model().volume < 1.0, "{mode:?}");
    }
}

#[test]
fn leaving_the_studio_ends_the_layer_editing() {
    let mut rig = Rig::new();
    rig.core.apply(Action::GoTo(AppMode::Studio));
    rig.core.apply(Action::Scene(
        dockview_domain::view::SceneAction::ToggleEdit,
    ));
    rig.core.apply(Action::GoTo(AppMode::Viewer));
    assert!(!rig.core.model().scenes.editing);
}
