//! The navigation row in `Core`, on fakes.

use std::time::Instant;

use dockview_domain::broadcast::StreamState;
use dockview_domain::frame::{ColorInfo, Frame, PixelFormat};
use dockview_domain::navigation::{EncoderInfo, EncoderState, NavTab};
use dockview_domain::recording::RecorderStatus;
use dockview_domain::view::Action;

use crate::testing::rig::Rig;

fn tab(rig: &Rig) -> NavTab {
    rig.core.model().nav.tab
}

#[test]
fn it_starts_on_the_viewer_with_the_panel_closed() {
    let rig = Rig::new();
    assert_eq!(tab(&rig), NavTab::Viewer);
    assert!(!rig.core.model().studio);
}

#[test]
fn the_studio_tab_opens_the_panel_and_the_viewer_tab_closes_it() {
    let mut rig = Rig::new();
    rig.core.apply(Action::GoTo(NavTab::Studio));
    assert!(rig.core.model().studio);
    assert_eq!(tab(&rig), NavTab::Studio);
    rig.core.apply(Action::GoTo(NavTab::Viewer));
    assert!(!rig.core.model().studio);
    assert_eq!(tab(&rig), NavTab::Viewer);
}

#[test]
fn recordings_and_settings_open_the_panel_and_stay_the_tab_that_is_on() {
    let mut rig = Rig::new();
    rig.core.apply(Action::GoTo(NavTab::Recordings));
    assert!(rig.core.model().studio);
    assert_eq!(tab(&rig), NavTab::Recordings);
    rig.core.apply(Action::GoTo(NavTab::Settings));
    assert_eq!(tab(&rig), NavTab::Settings);
    rig.core.apply(Action::GoTo(NavTab::Studio));
    assert_eq!(tab(&rig), NavTab::Studio);
}

#[test]
fn the_panel_comes_back_on_the_last_tab_and_full_screen_is_the_viewer() {
    let mut rig = Rig::new();
    rig.core.apply(Action::GoTo(NavTab::Settings));
    rig.core.apply(Action::GoTo(NavTab::Viewer));
    rig.core.apply(Action::ToggleStudio);
    assert_eq!(tab(&rig), NavTab::Settings, "the tab it was on");
    rig.core.apply(Action::ToggleFullscreen);
    assert_eq!(
        tab(&rig),
        NavTab::Viewer,
        "the panel is hidden in full screen"
    );
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
fn pictures_the_viewer_never_showed_are_counted_as_lost() {
    let rig = Rig::new();
    assert_eq!(rig.core.model().nav.drops, 0);
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
    assert_eq!(rig.core.model().nav.drops, 1);
}
