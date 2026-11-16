//! Tests of the Stream section of the Studio panel.

use super::*;
use dockview_domain::broadcast::StreamState;
use dockview_domain::i18n::Lang;
use dockview_domain::view::SceneAction;

use super::super::stream::{service_of, FPS_CHOICES, KBPS_CHOICES, SERVICES};

fn stream_open() -> Rig {
    let mut rig = Rig::new();
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Stream));
    rig
}

fn press_key(rig: &mut Rig, key: TextKey) {
    rig.panel.key(key);
    rig.frame(0.016);
}

fn type_text(rig: &mut Rig, text: &str) {
    press_key(rig, TextKey::Text(text.into()));
}

#[test]
fn the_section_is_between_the_recording_and_the_statistics() {
    let all = StudioSection::ALL;
    let at = |s: StudioSection| all.iter().position(|x| *x == s).unwrap();
    assert_eq!(at(StudioSection::Stream), at(StudioSection::Recording) + 1);
    assert_eq!(at(StudioSection::Stats), at(StudioSection::Stream) + 1);
    assert_eq!(
        StudioSection::from_name("stream"),
        Some(StudioSection::Stream)
    );
}

#[test]
fn it_offers_the_services_the_quality_and_the_button_with_their_actions() {
    let rig = stream_open();
    let mut expected: Vec<Action> = SERVICES
        .iter()
        .map(|(_, url)| Action::SetStreamUrl((*url).to_owned()))
        .collect();
    expected.push(Action::SetStreamUrl(String::new())); // another service
    expected.extend(FPS_CHOICES.iter().map(|f| Action::SetStreamFps(*f)));
    expected.extend(KBPS_CHOICES.iter().map(|k| Action::SetStreamKbps(*k)));
    expected.push(Action::ToggleStream);
    assert_eq!(rig.panel.items, expected);
}

#[test]
fn clicking_a_service_and_the_button_ask_for_them() {
    let mut rig = stream_open();
    rig.click_on(PanelHit::Item(0));
    assert_eq!(
        rig.panel.take_actions(),
        [Action::SetStreamUrl(SERVICES[0].1.to_owned())]
    );
    let last = rig.panel.items.len() - 1;
    rig.click_on(PanelHit::Item(last));
    assert_eq!(rig.panel.take_actions(), [Action::ToggleStream]);
}

#[test]
fn the_chosen_service_follows_the_address() {
    assert_eq!(service_of("rtmps://live.twitch.tv:443/app"), 0);
    assert_eq!(service_of("rtmp://a.rtmp.youtube.com/live2"), 1);
    assert_eq!(service_of(SERVICES[2].1), 2);
    assert_eq!(service_of(""), SERVICES.len());
    assert_eq!(service_of("rtmp://example.org/live"), SERVICES.len());
}

#[test]
fn the_address_is_typed_in_its_field_and_accepted_with_enter() {
    let mut rig = stream_open();
    rig.model.stream.address = "rtmp://old.example/app".into();
    rig.frame(0.016);
    rig.click_on(PanelHit::StreamUrl);
    assert!(rig.panel.is_editing(), "the keys now go to the field");
    // It starts from what is there; Ctrl-A style replacing is not needed.
    for _ in 0.."rtmp://old.example/app".len() {
        press_key(&mut rig, TextKey::Backspace);
    }
    type_text(&mut rig, "rtmps://live.example:443/app");
    press_key(&mut rig, TextKey::Enter);
    assert_eq!(
        rig.panel.take_actions(),
        [Action::SetStreamUrl("rtmps://live.example:443/app".into())]
    );
    assert!(!rig.panel.is_editing());
}

#[test]
fn the_key_starts_empty_even_when_one_is_saved_and_is_never_shown() {
    let mut rig = stream_open();
    rig.model.stream.has_key = true;
    rig.frame(0.016);
    rig.click_on(PanelHit::StreamKey);
    type_text(&mut rig, "live_123_secret");
    // What is shown while typing is dots, not the key.
    let key_chars = "live_123_secret".chars().count();
    assert_eq!(
        rig.panel
            .editing
            .as_ref()
            .map(|(_, e)| e.text().chars().count()),
        Some(key_chars)
    );
    press_key(&mut rig, TextKey::Enter);
    assert_eq!(
        rig.panel.take_actions(),
        [Action::SetStreamKey("live_123_secret".into())]
    );
}

#[test]
fn escape_gives_up_the_typing_and_asks_for_nothing() {
    let mut rig = stream_open();
    rig.click_on(PanelHit::StreamKey);
    type_text(&mut rig, "oops");
    press_key(&mut rig, TextKey::Escape);
    assert!(!rig.panel.is_editing());
    assert!(rig.panel.take_actions().is_empty());
}

#[test]
fn a_press_elsewhere_ends_the_typing_without_keeping_it() {
    let mut rig = stream_open();
    rig.click_on(PanelHit::StreamUrl);
    type_text(&mut rig, "x");
    rig.click_on(PanelHit::Header(StudioSection::Audio));
    assert!(!rig.panel.is_editing());
    assert!(rig.panel.take_actions().is_empty());
}

#[test]
fn a_press_inside_the_field_being_typed_keeps_what_was_typed() {
    let mut rig = stream_open();
    rig.click_on(PanelHit::StreamKey);
    type_text(&mut rig, "abc");
    rig.click_on(PanelHit::StreamKey);
    assert_eq!(
        rig.panel.editing.as_ref().map(|(_, e)| e.text().to_owned()),
        Some("abc".into())
    );
}

#[test]
fn only_one_line_is_typed_at_a_time_and_the_scene_name_still_works() {
    let mut rig = stream_open();
    rig.click_on(PanelHit::StreamUrl);
    rig.click_on(PanelHit::StreamKey);
    assert!(
        matches!(
            rig.panel.editing.as_ref().map(|(t, _)| *t),
            Some(super::super::edit::EditTarget::StreamKey)
        ) || rig.panel.editing.is_none()
    );
    let _ = SceneAction::New; // the scene actions are untouched by the section
}

#[test]
fn the_status_says_what_the_stream_is_doing_in_both_languages() {
    for lang in [Lang::En, Lang::Es] {
        for state in [
            StreamState::Offline,
            StreamState::Connecting,
            StreamState::Live,
            StreamState::Reconnecting,
            StreamState::Failed,
        ] {
            let mut rig = stream_open();
            rig.model.lang = lang;
            rig.model.stream.state = state;
            rig.model.stream.bitrate_kbps = 4400;
            rig.frame(0.016);
            assert!(!rig.ui.vertices().is_empty(), "{lang:?} {state:?}");
        }
    }
}

#[test]
fn the_body_is_as_tall_as_its_content() {
    let m = model();
    let h = section_height(StudioSection::Stream, &m);
    assert!(h > 300.0 && h < 700.0, "{h}");
}
