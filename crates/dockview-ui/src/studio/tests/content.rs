//! Tests of what is inside each section of the Studio panel.

use super::*;

// -- The Settings section -------------------------------------------------

use dockview_domain::settings::{LangMode, RenderPreset, SceneTransition, VideoModePref};
use dockview_domain::theme::ThemeMode;

fn settings_open() -> Rig {
    let mut rig = Rig::new();
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Settings));
    rig
}

#[test]
fn folded_the_settings_section_registers_no_controls() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    assert!(rig.panel.items.is_empty());
}

#[test]
fn the_settings_offer_the_same_choices_and_actions_as_the_popover() {
    let rig = settings_open();
    assert_eq!(
        rig.panel.items,
        [
            Action::SetPreset(RenderPreset::Performance),
            Action::SetPreset(RenderPreset::Balanced),
            Action::SetPreset(RenderPreset::Quality),
            Action::SetVideoMode(VideoModePref::Auto),
            Action::SetVideoMode(VideoModePref::Compressed),
            Action::SetSceneTransition(SceneTransition::Smooth),
            Action::SetSceneTransition(SceneTransition::Cut),
            Action::SetTheme(ThemeMode::Auto),
            Action::SetTheme(ThemeMode::Dark),
            Action::SetTheme(ThemeMode::Light),
            Action::SetLang(LangMode::Auto),
            Action::SetLang(LangMode::En),
            Action::SetLang(LangMode::Es),
        ]
    );
}

#[test]
fn clicking_a_choice_asks_for_it() {
    let mut rig = settings_open();
    // Dark is the ninth control (see the order above).
    rig.click_on(PanelHit::Item(8));
    assert_eq!(
        rig.panel.take_actions(),
        [Action::SetTheme(ThemeMode::Dark)]
    );
    rig.click_on(PanelHit::Item(12));
    assert_eq!(rig.panel.take_actions(), [Action::SetLang(LangMode::Es)]);
    rig.click_on(PanelHit::Item(6));
    assert_eq!(
        rig.panel.take_actions(),
        [Action::SetSceneTransition(SceneTransition::Cut)]
    );
    rig.click_on(PanelHit::Item(2));
    assert_eq!(
        rig.panel.take_actions(),
        [Action::SetPreset(RenderPreset::Quality)]
    );
}

#[test]
fn the_indicator_follows_what_the_model_says_is_selected() {
    let mut rig = settings_open();
    rig.model.theme_mode = ThemeMode::Light;
    rig.model.preset = RenderPreset::Quality;
    rig.model.lang_mode = LangMode::Es;
    rig.model.video_mode = VideoModePref::Compressed;
    rig.model.scenes.transition = SceneTransition::Cut;
    rig.frame(0.016);
    assert_eq!(rig.panel.slide["scene-transition"], 1.0);
    assert_eq!(rig.panel.slide["theme"], 2.0);
    assert_eq!(rig.panel.slide["quality"], 2.0);
    assert_eq!(rig.panel.slide["language"], 2.0);
    assert_eq!(rig.panel.slide["video-mode"], 1.0);
}

#[test]
fn the_indicator_glides_to_a_new_choice_when_animations_are_on() {
    let mut rig = settings_open();
    rig.panel.set_animations(true);
    rig.frame(0.016);
    rig.model.theme_mode = ThemeMode::Light;
    rig.frame(0.016);
    let half_way = rig.panel.slide["theme"];
    assert!(half_way > 0.0 && half_way < 2.0, "{half_way}");
    for _ in 0..80 {
        rig.frame(0.016);
    }
    assert_eq!(rig.panel.slide["theme"], 2.0);
}

#[test]
fn the_settings_body_is_as_tall_as_its_content() {
    use super::settings::body_height;
    use super::widgets::{GAP, LABEL_H, ROW_H};
    let groups = 5.0 * (LABEL_H + ROW_H) + 4.0 * GAP;
    assert!(body_height() > groups, "room above and below");
    assert!(body_height() < groups + 40.0, "but not much");
    assert_eq!(
        section_height(StudioSection::Settings, &model()),
        body_height()
    );
}

#[test]
fn the_settings_controls_stay_inside_the_panel() {
    let rig = settings_open();
    let area = rig.panel.area().unwrap();
    for (hit, rect) in &rig.panel.hits {
        if matches!(hit, PanelHit::Item(_)) {
            assert!(
                rect.x >= area.x && rect.x + rect.w <= area.x + area.w,
                "{rect:?}"
            );
        }
    }
}

#[test]
fn the_texts_of_the_settings_come_in_the_language_of_the_model() {
    use dockview_domain::i18n::{tr, Key, Lang};
    assert_ne!(
        tr(Lang::En, Key::PresetPerformance),
        tr(Lang::Es, Key::PresetPerformance)
    );
    let mut rig = settings_open();
    rig.model.lang = Lang::Es;
    rig.frame(0.016);
    assert!(!rig.ui.vertices().is_empty());
}

#[test]
fn a_section_can_be_unfolded_from_the_start() {
    let mut rig = Rig::new();
    rig.panel.unfold(StudioSection::Audio);
    rig.frame(0.016);
    assert!(rig.panel.is_open(StudioSection::Audio));
    assert_eq!(rig.panel.amount[2], 1.0, "already open, not sliding open");
}

// -- The Sources section --------------------------------------------------

use dockview_domain::view::{CurrentSource, SourcesView};

fn sources_open() -> Rig {
    let mut rig = Rig::new();
    rig.model.sources = SourcesView {
        monitors: vec!["Screen 1 · 1920×1080 (primary)".into()],
        windows: vec!["A game".into(), "A browser".into()],
        current: CurrentSource::Capture,
    };
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Sources));
    rig
}

#[test]
fn the_sources_offer_every_card_screen_and_window_with_the_popovers_actions() {
    let rig = sources_open();
    assert_eq!(
        rig.panel.items,
        [
            Action::SelectVideo(0),
            Action::SelectVideo(1),
            Action::SelectMonitor(0),
            Action::SelectWindow(0),
            Action::SelectWindow(1),
        ]
    );
}

#[test]
fn clicking_a_source_asks_for_it() {
    let mut rig = sources_open();
    rig.click_on(PanelHit::Item(1));
    assert_eq!(rig.panel.take_actions(), [Action::SelectVideo(1)]);
    rig.click_on(PanelHit::Item(2));
    assert_eq!(rig.panel.take_actions(), [Action::SelectMonitor(0)]);
    rig.click_on(PanelHit::Item(4));
    assert_eq!(rig.panel.take_actions(), [Action::SelectWindow(1)]);
}

#[test]
fn the_body_is_as_tall_as_what_there_is_to_list() {
    use super::sources::body_height;
    let mut m = model();
    let two_cards = body_height(&m);
    m.video_devices.push("Another card".into());
    assert!(body_height(&m) > two_cards, "a third card is a third row");
    // Only so many windows are listed, however many there are.
    m.sources.windows = (0..20).map(|i| format!("Window {i}")).collect();
    let capped = body_height(&m);
    m.sources.windows = (0..8).map(|i| format!("Window {i}")).collect();
    assert_eq!(body_height(&m), capped);
    assert_eq!(section_height(StudioSection::Sources, &m), body_height(&m));
}

#[test]
fn an_empty_list_takes_one_row_for_its_note_and_offers_nothing() {
    use super::widgets::list_height;
    assert_eq!(list_height(0, 5), list_height(1, 5));
    let mut rig = Rig::new();
    rig.model.video_devices.clear();
    rig.model.video_selected = None;
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Sources));
    assert!(
        rig.panel.items.is_empty(),
        "no cards, no screens, no windows"
    );
}

#[test]
fn a_long_window_title_does_not_overflow_the_panel() {
    let mut rig = sources_open();
    rig.model.sources.windows = vec!["A very long title ".repeat(12)];
    rig.frame(0.016);
    let area = rig.panel.area().unwrap();
    assert!(rig
        .ui
        .vertices()
        .iter()
        .all(|v| v.pos[0] <= area.x + area.w + 4.0));
}

// -- The Audio section ----------------------------------------------------

use dockview_domain::view::{MicState, MicView};

fn audio_open() -> Rig {
    let mut rig = Rig::new();
    rig.model.volume = 0.2;
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Audio));
    rig
}

#[test]
fn the_audio_controls_ask_for_what_the_pill_and_the_popovers_ask_for() {
    let rig = audio_open();
    assert_eq!(
        rig.panel.items,
        [
            Action::SetSync(-10),
            Action::SetSync(10),
            Action::SelectAudio(0),
            Action::ToggleMic,
            Action::SelectMic(0),
            Action::SelectMic(1),
        ]
    );
}

#[test]
fn the_sync_buttons_stop_at_the_ends_of_the_range() {
    let mut rig = audio_open();
    rig.model.sync_ms = 300;
    rig.frame(0.016);
    assert_eq!(
        rig.panel.items[1],
        Action::SetSync(300),
        "no further than +300"
    );
    rig.model.sync_ms = -300;
    rig.frame(0.016);
    assert_eq!(rig.panel.items[0], Action::SetSync(-300));
}

#[test]
fn clicking_the_microphone_or_an_input_asks_for_it() {
    let mut rig = audio_open();
    rig.click_on(PanelHit::Item(3));
    assert_eq!(rig.panel.take_actions(), [Action::ToggleMic]);
    rig.click_on(PanelHit::Item(5));
    assert_eq!(rig.panel.take_actions(), [Action::SelectMic(1)]);
    rig.click_on(PanelHit::Item(1));
    assert_eq!(rig.panel.take_actions(), [Action::SetSync(10)]);
}

#[test]
fn dragging_the_volume_slider_follows_the_pointer_and_stops_when_released() {
    let mut rig = audio_open();
    let track = rig.panel.sliders[0].track;
    let at = |share: f32| track.x + track.w * share;

    rig.move_to(at(0.75), track.y);
    rig.pointer.press();
    rig.frame(0.016);
    assert_eq!(rig.panel.take_actions(), [Action::SetVolume(0.75)]);

    // Held and moved: it follows (the model has not answered yet).
    rig.pointer.pos = Some((at(0.4), track.y + 30.0));
    rig.frame(0.016);
    assert_eq!(rig.panel.take_actions(), [Action::SetVolume(0.4)]);
    // Past either end it stops at the end.
    rig.pointer.pos = Some((at(2.0), track.y));
    rig.frame(0.016);
    assert_eq!(rig.panel.take_actions(), [Action::SetVolume(1.0)]);
    rig.pointer.pos = Some((at(-1.0), track.y));
    rig.frame(0.016);
    assert_eq!(rig.panel.take_actions(), [Action::SetVolume(0.0)]);

    rig.pointer.release();
    rig.frame(0.016);
    rig.pointer.pos = Some((at(0.5), track.y));
    rig.frame(0.016);
    assert!(
        rig.panel.take_actions().is_empty(),
        "let go: nothing follows"
    );
}

#[test]
fn a_slider_that_is_not_dragged_asks_for_nothing_and_a_click_on_the_value_does_not_repeat() {
    let mut rig = audio_open();
    for _ in 0..5 {
        rig.frame(0.016);
    }
    assert!(rig.panel.take_actions().is_empty());
    let track = rig.panel.sliders[0].track;
    // Pressing exactly where the slider already is changes nothing.
    rig.move_to(track.x + track.w * 0.2, track.y);
    rig.pointer.press();
    rig.frame(0.016);
    assert!(rig.panel.take_actions().is_empty());
}

#[test]
fn the_level_meter_shows_only_while_the_microphone_is_open() {
    let mut rig = audio_open();
    rig.frame(0.016);
    let off = rig.ui.vertices().len();
    rig.model.mic = MicView {
        state: MicState::Live,
        level: 0.6,
    };
    rig.frame(0.016);
    assert!(rig.ui.vertices().len() > off, "the level bar is drawn");
    rig.model.mic = MicView {
        state: MicState::Unavailable,
        level: 0.0,
    };
    rig.frame(0.016);
    assert!(!rig.ui.vertices().is_empty());
}

#[test]
fn the_switch_glides_to_its_new_state_when_animations_are_on() {
    let mut rig = audio_open();
    rig.panel.set_animations(true);
    rig.frame(0.016);
    assert_eq!(rig.panel.slide["mic"], 0.0);
    rig.model.mic = MicView {
        state: MicState::Live,
        level: 0.0,
    };
    rig.frame(0.016);
    let half = rig.panel.slide["mic"];
    assert!(half > 0.0 && half < 1.0, "{half}");
    for _ in 0..60 {
        rig.frame(0.016);
    }
    assert_eq!(rig.panel.slide["mic"], 1.0);
}

#[test]
fn the_audio_body_grows_with_the_inputs_it_lists() {
    use super::audio::body_height;
    let mut m = model();
    let before = body_height(&m);
    m.audio_devices.push("Another input".into());
    m.mic_devices.push("Another microphone".into());
    assert!(body_height(&m) > before);
    assert_eq!(section_height(StudioSection::Audio, &m), body_height(&m));
}

// -- The Statistics section -----------------------------------------------

use dockview_domain::diagnostics::Finding;

fn some_stats(findings: Vec<Finding>) -> Stats {
    Stats {
        device_mode: Some("NV12 · 1920×1080 · 60fps".into()),
        received_fps: 60.0,
        shown_fps: 60.0,
        latency_ms: Some(2.2),
        measured: true,
        findings,
        ..Stats::default()
    }
}

#[test]
fn the_statistics_body_grows_with_the_diagnosis_and_with_narrower_panels() {
    use super::stats::body_height;
    let m = model();
    let mut ui = Ui::new();
    ui.begin(WINDOW, 1.0);
    let one = body_height(
        &mut ui,
        &m,
        &some_stats(vec![Finding::Healthy]),
        PANEL_WIDTH,
    );
    let two = body_height(
        &mut ui,
        &m,
        &some_stats(vec![Finding::Healthy, Finding::DroppingFrames]),
        PANEL_WIDTH,
    );
    assert!(two > one, "a second finding takes room");
    // The same long sentence wraps onto more lines in a narrower panel.
    let long = some_stats(vec![Finding::CaptureBelowMode]);
    let wide = body_height(&mut ui, &m, &long, PANEL_WIDTH);
    let narrow = body_height(&mut ui, &m, &long, 200.0);
    assert!(narrow > wide, "{narrow} {wide}");
}

#[test]
fn the_statistics_text_stays_inside_its_body() {
    let mut rig = Rig::new();
    rig.window = (1280, 1400); // tall enough to see all of it without scrolling
    let stats = some_stats(vec![
        Finding::Healthy,
        Finding::DroppingFrames,
        Finding::HighCpu,
    ]);
    rig.ui.begin(rig.window, 1.0);
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Stats));
    // The last section: its body starts under its header.
    let top = TOP_H + HEADER_H * 5.0 + HEADER_H;
    let height = StudioSection::Stats.body_height(&mut Ui::new(), &rig.model, &stats, PANEL_WIDTH);
    // Draw with those statistics.
    rig.ui.begin(rig.window, 1.0);
    let layout = studio_layout(rig.window, 1.0, 1.0);
    let area = layout.panel.map(|r| Rect::new(r.x, r.y, r.w, r.h));
    rig.panel.frame(
        &mut rig.ui,
        &rig.model,
        &stats,
        area,
        PANEL_WIDTH,
        &rig.pointer,
        0.016,
    );
    let glyphs: Vec<_> = rig
        .ui
        .vertices()
        .iter()
        .filter(|v| v.shape[2] < 0.0 && v.pos[1] > top)
        .collect();
    assert!(!glyphs.is_empty());
    assert!(
        glyphs.iter().all(|v| v.pos[1] <= top + height + 1.0),
        "text below the end of the body"
    );
}

#[test]
fn the_statistics_have_nothing_to_click() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Stats));
    assert!(rig.panel.items.is_empty());
    assert!(rig.panel.sliders.is_empty());
}

#[test]
fn an_unmeasured_run_still_draws_its_rows() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    let closed = rig.ui.vertices().len();
    rig.click_on(PanelHit::Header(StudioSection::Stats));
    assert!(
        rig.ui.vertices().len() > closed,
        "the rows and the diagnosis show"
    );
}

#[test]
fn a_long_value_never_overlaps_its_label() {
    let mut rig = Rig::new();
    rig.window = (1280, 1400);
    let mut stats = some_stats(vec![Finding::Healthy]);
    stats.device_mode = Some("NV12 · 1920×1080 · 60fps · a very long mode name indeed".into());
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Stats));
    rig.ui.begin(rig.window, 1.0);
    let layout = studio_layout(rig.window, 1.0, 1.0);
    let area = layout.panel.map(|r| Rect::new(r.x, r.y, r.w, r.h));
    rig.panel.frame(
        &mut rig.ui,
        &rig.model,
        &stats,
        area,
        PANEL_WIDTH,
        &rig.pointer,
        0.016,
    );
    // No glyph goes past the panel's right edge, however long the value is.
    let right = area.unwrap().x + PANEL_WIDTH;
    assert!(rig.ui.vertices().iter().all(|v| v.pos[0] <= right + 4.0));
}

// -- The Scenes section ---------------------------------------------------

use dockview_domain::view::{LayerView, SceneAction, ScenesView};

fn scene(action: SceneAction) -> Action {
    Action::Scene(action)
}

fn layer(name: &str, visible: bool) -> LayerView {
    LayerView {
        kind: Default::default(),
        locked: false,
        name: name.into(),
        visible,
        rect: dockview_domain::scene::NormRect::FULL,
    }
}

fn scenes_open(view: ScenesView) -> Rig {
    let mut rig = Rig::new();
    rig.window = (1280, 1600); // room for every list without scrolling
    rig.model.scenes = view;
    rig.model.sources.windows = vec!["A game".into()];
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Scenes));
    rig
}

fn two_scenes_second_in_use() -> ScenesView {
    ScenesView {
        names: vec!["Scene 1".into(), "Scene 2".into()],
        active: Some(1),
        layers: vec![layer("UGREEN 25173", true), layer("Screen 1", false)],
        can_add: true,
        ..ScenesView::default()
    }
}

#[test]
fn the_scenes_offer_what_the_popover_offers_with_the_same_actions() {
    let rig = scenes_open(two_scenes_second_in_use());
    assert_eq!(
        rig.panel.items,
        [
            scene(SceneAction::Activate(None)),
            scene(SceneAction::Activate(Some(0))),
            scene(SceneAction::Activate(Some(1))),
            scene(SceneAction::New),
            scene(SceneAction::ToggleEdit),
            scene(SceneAction::SelectLayer(Some(0))),
            scene(SceneAction::ToggleLayer(0)),
            scene(SceneAction::SelectLayer(Some(1))),
            scene(SceneAction::ToggleLayer(1)),
            scene(SceneAction::RemoveLayer(1)),
            scene(SceneAction::AddCapture(0)),
            scene(SceneAction::AddCapture(1)),
            scene(SceneAction::AddWindow(0)),
        ]
    );
}

#[test]
fn deleting_a_scene_asks_first_and_only_the_yes_deletes_it() {
    let mut rig = scenes_open(two_scenes_second_in_use());
    rig.click_on(PanelHit::AskDelete);
    assert!(
        rig.panel.take_actions().is_empty(),
        "asking deletes nothing"
    );
    rig.frame(0.016);
    // The question is on screen with a button for each answer.
    rig.click_on(PanelHit::CancelDelete);
    assert!(rig.panel.take_actions().is_empty(), "no deletes nothing");
    rig.frame(0.016);
    rig.click_on(PanelHit::AskDelete);
    rig.frame(0.016);
    let yes = rig
        .panel
        .items
        .iter()
        .position(|a| *a == scene(SceneAction::Delete(1)))
        .expect("the yes button");
    rig.click_on(PanelHit::Item(yes));
    assert_eq!(rig.panel.take_actions(), [scene(SceneAction::Delete(1))]);
}

#[test]
fn the_question_goes_away_after_the_answer_and_with_another_scene() {
    let mut rig = scenes_open(two_scenes_second_in_use());
    rig.click_on(PanelHit::AskDelete);
    rig.frame(0.016);
    assert!(rig.panel.confirm_delete.is_some());
    // Another scene is chosen: the question was about the old one.
    rig.model.scenes.active = Some(0);
    rig.frame(0.016);
    assert_eq!(rig.panel.confirm_delete, None);
    rig.frame(0.016);
    assert!(
        rig.panel
            .hits
            .iter()
            .any(|(h, _)| *h == PanelHit::AskDelete),
        "the plain button is back"
    );
}

/// Starts renaming the scene in use and types `text` after what was there.
fn start_renaming(rig: &mut Rig) {
    rig.click_on(PanelHit::Rename);
    rig.frame(0.016);
    assert!(rig.panel.is_editing());
}

fn type_text(rig: &mut Rig, text: &str) {
    rig.panel.key(TextKey::Text(text.into()));
    rig.frame(0.016);
}

#[test]
fn renaming_starts_from_the_current_name_and_enter_asks_for_the_new_one() {
    let mut rig = scenes_open(two_scenes_second_in_use());
    assert!(!rig.panel.is_editing());
    start_renaming(&mut rig);
    for _ in 0..3 {
        rig.panel.key(TextKey::Backspace);
    }
    type_text(&mut rig, "ñ 9");
    rig.panel.key(TextKey::Enter);
    rig.frame(0.016);
    assert_eq!(
        rig.panel.take_actions(),
        [Action::RenameScene(1, "Scenñ 9".into())]
    );
    assert!(!rig.panel.is_editing());
}

#[test]
fn escape_gives_up_the_rename_and_asks_for_nothing() {
    let mut rig = scenes_open(two_scenes_second_in_use());
    start_renaming(&mut rig);
    type_text(&mut rig, "x");
    rig.panel.key(TextKey::Escape);
    rig.frame(0.016);
    assert!(!rig.panel.is_editing());
    assert!(rig.panel.take_actions().is_empty());
}

#[test]
fn a_press_outside_the_field_ends_the_rename_but_one_inside_does_not() {
    let mut rig = scenes_open(two_scenes_second_in_use());
    start_renaming(&mut rig);
    rig.frame(0.016);
    rig.click_on(PanelHit::EditField);
    assert!(rig.panel.is_editing(), "a click in the field keeps editing");
    rig.click_on(PanelHit::Header(StudioSection::Scenes));
    assert!(!rig.panel.is_editing());
    assert!(rig.panel.take_actions().is_empty());
}

#[test]
fn the_name_cannot_grow_past_its_limit_or_its_field() {
    let mut rig = scenes_open(two_scenes_second_in_use());
    start_renaming(&mut rig);
    type_text(&mut rig, &"W".repeat(60));
    rig.panel.key(TextKey::Enter);
    rig.frame(0.016);
    let [Action::RenameScene(_, name)] = &rig.panel.take_actions()[..] else {
        panic!("one rename was expected");
    };
    assert!(name.chars().count() <= dockview_domain::scene::MAX_SCENE_NAME_CHARS);
    let width = rig.ui.measure(name, super::super::scenes::ROW_TEXT).0;
    assert!(
        width <= rig.panel.edit_room,
        "{width} does not fit the field"
    );
}

#[test]
fn keys_are_only_kept_while_a_name_is_being_typed() {
    let mut rig = scenes_open(two_scenes_second_in_use());
    rig.panel.key(TextKey::Text("x".into()));
    rig.frame(0.016);
    start_renaming(&mut rig);
    rig.panel.key(TextKey::Enter);
    rig.frame(0.016);
    // What was typed before the field existed did not end up in it.
    assert_eq!(
        rig.panel.take_actions(),
        [Action::RenameScene(1, "Scene 2".into())]
    );
}

#[test]
fn choosing_another_scene_or_closing_the_panel_ends_the_rename() {
    let mut rig = scenes_open(two_scenes_second_in_use());
    start_renaming(&mut rig);
    rig.model.scenes.active = Some(0);
    rig.frame(0.016);
    assert!(!rig.panel.is_editing());

    let mut rig = scenes_open(two_scenes_second_in_use());
    start_renaming(&mut rig);
    rig.visible = false;
    rig.frame(0.016);
    assert!(!rig.panel.is_editing());
}

fn three_layers_with_the_middle_one_chosen() -> ScenesView {
    ScenesView {
        names: vec!["Scene".into()],
        active: Some(0),
        layers: vec![
            layer("Base", true),
            layer("Middle", true),
            layer("Top", true),
        ],
        can_add: false,
        selected: Some(1),
        ..ScenesView::default()
    }
}

fn index_of(rig: &Rig, action: SceneAction) -> Option<usize> {
    rig.panel.items.iter().position(|a| *a == scene(action))
}

#[test]
fn a_layer_moves_up_and_down_unless_it_is_at_an_end_and_the_base_never_moves() {
    let rig = scenes_open(three_layers_with_the_middle_one_chosen());
    assert!(index_of(&rig, SceneAction::MoveLayer(1, 1)).is_some());
    assert!(index_of(&rig, SceneAction::MoveLayer(2, -1)).is_some());
    // The first over the base cannot go lower, the top one cannot go higher,
    // and the base has no buttons.
    assert!(index_of(&rig, SceneAction::MoveLayer(1, -1)).is_none());
    assert!(index_of(&rig, SceneAction::MoveLayer(2, 1)).is_none());
    assert!(index_of(&rig, SceneAction::MoveLayer(0, 1)).is_none());
    assert!(index_of(&rig, SceneAction::MoveLayer(0, -1)).is_none());
}

#[test]
fn clicking_a_layer_chooses_it_and_the_edit_button_switches_the_editor() {
    let mut rig = scenes_open(three_layers_with_the_middle_one_chosen());
    let pick = index_of(&rig, SceneAction::SelectLayer(Some(2))).unwrap();
    rig.click_on(PanelHit::Item(pick));
    assert_eq!(
        rig.panel.take_actions(),
        [scene(SceneAction::SelectLayer(Some(2)))]
    );
    let edit = index_of(&rig, SceneAction::ToggleEdit).unwrap();
    rig.click_on(PanelHit::Item(edit));
    assert_eq!(rig.panel.take_actions(), [scene(SceneAction::ToggleEdit)]);
}

#[test]
fn the_buttons_of_a_layer_do_not_also_choose_it() {
    let mut rig = scenes_open(three_layers_with_the_middle_one_chosen());
    let eye = index_of(&rig, SceneAction::ToggleLayer(2)).unwrap();
    rig.click_on(PanelHit::Item(eye));
    assert_eq!(
        rig.panel.take_actions(),
        [scene(SceneAction::ToggleLayer(2))]
    );
}

#[test]
fn the_base_layer_can_be_hidden_but_not_removed() {
    let rig = scenes_open(two_scenes_second_in_use());
    assert!(rig
        .panel
        .items
        .contains(&scene(SceneAction::ToggleLayer(0))));
    assert!(!rig
        .panel
        .items
        .contains(&scene(SceneAction::RemoveLayer(0))));
}

#[test]
fn without_a_scene_in_use_there_are_no_layers_and_nothing_to_delete() {
    let rig = scenes_open(ScenesView {
        names: vec!["Scene 1".into()],
        active: None,
        layers: vec![],
        can_add: false,
        ..ScenesView::default()
    });
    assert_eq!(
        rig.panel.items,
        [
            scene(SceneAction::Activate(None)),
            scene(SceneAction::Activate(Some(0))),
            scene(SceneAction::New),
        ]
    );
}

#[test]
fn a_full_scene_offers_no_more_sources_to_add() {
    let mut view = two_scenes_second_in_use();
    view.can_add = false;
    let rig = scenes_open(view);
    assert!(!rig.panel.items.iter().any(|a| matches!(
        a,
        Action::Scene(SceneAction::AddCapture(_) | SceneAction::AddWindow(_))
    )));
}

#[test]
fn the_buttons_ask_for_what_they_say() {
    let mut rig = scenes_open(two_scenes_second_in_use());
    rig.click_on(PanelHit::Item(3));
    assert_eq!(rig.panel.take_actions(), [scene(SceneAction::New)]);
    let remove = rig
        .panel
        .items
        .iter()
        .position(|a| *a == scene(SceneAction::RemoveLayer(1)))
        .unwrap();
    rig.click_on(PanelHit::Item(remove));
    assert_eq!(
        rig.panel.take_actions(),
        [scene(SceneAction::RemoveLayer(1))]
    );
    rig.click_on(PanelHit::Item(0));
    assert_eq!(
        rig.panel.take_actions(),
        [scene(SceneAction::Activate(None))]
    );
}

#[test]
fn the_scenes_body_grows_with_the_scenes_and_the_layers() {
    use super::scenes::body_height;
    let mut m = model();
    let empty = body_height(&m);
    m.scenes = two_scenes_second_in_use();
    let in_use = body_height(&m);
    assert!(in_use > empty, "layers and sources to add take room");
    m.scenes.layers.push(layer("Window", true));
    assert!(body_height(&m) > in_use);
    m.scenes.can_add = false;
    assert!(
        body_height(&m) < in_use + 40.0,
        "no add lists when it is full"
    );
    assert_eq!(section_height(StudioSection::Scenes, &m), body_height(&m));
}

#[test]
fn a_long_layer_name_does_not_overflow_the_panel() {
    let mut view = two_scenes_second_in_use();
    view.layers[1].name = "A window with a really long title ".repeat(8);
    let rig = scenes_open(view);
    let area = rig.panel.area().unwrap();
    assert!(rig
        .ui
        .vertices()
        .iter()
        .all(|v| v.pos[0] <= area.x + area.w + 4.0));
}

// -- The Recording section ------------------------------------------------

use dockview_domain::settings::RecordingQuality;
use dockview_domain::view::{RecState, RecView, ReplayState};

fn recording_open() -> Rig {
    let mut rig = Rig::new();
    rig.window = (1280, 1400); // room for all of it without scrolling
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Recording));
    rig
}

#[test]
fn the_recording_controls_ask_for_what_the_pill_and_the_replay_popover_ask_for() {
    let rig = recording_open();
    assert_eq!(
        rig.panel.items,
        [
            Action::ToggleReplay,
            Action::ReplayLength(-1),
            Action::ReplayLength(1),
            Action::SetQuality(RecordingQuality::Normal),
            Action::SetQuality(RecordingQuality::High),
            Action::SaveReplay,
            Action::ToggleRecord,
            Action::OpenReplayFolder,
            Action::ChooseFolder,
        ]
    );
}

#[test]
fn clicking_each_control_asks_for_it() {
    let mut rig = recording_open();
    for (item, action) in [
        (0, Action::ToggleReplay),
        (2, Action::ReplayLength(1)),
        (4, Action::SetQuality(RecordingQuality::High)),
        (5, Action::SaveReplay),
        (6, Action::ToggleRecord),
        (8, Action::ChooseFolder),
    ] {
        rig.click_on(PanelHit::Item(item));
        assert_eq!(rig.panel.take_actions(), [action]);
    }
}

#[test]
fn the_record_button_says_what_it_will_do() {
    use super::recording::record_label;
    let mut m = model();
    assert_eq!(record_label(&m), "Record");
    m.rec = RecView {
        state: RecState::Recording,
        seconds: 12.4,
    };
    let label = record_label(&m);
    assert!(label.starts_with("Stop recording"), "{label}");
    assert!(label.contains("12 s"), "{label}");
}

#[test]
fn the_status_and_the_hint_say_what_the_replay_holds_and_how_to_save() {
    use super::recording::{hint_sentence, status_sentence};
    let mut m = model();
    assert_eq!(status_sentence(&m), tr(m.lang, Key::ReplayOff));
    m.replay.state = ReplayState::Recording;
    m.replay.buffered_seconds = 12.7;
    m.replay.buffered_mb = 80.0;
    let status = status_sentence(&m);
    assert!(
        status.contains("12 s") && status.contains("80 MB"),
        "{status}"
    );
    assert!(
        hint_sentence(&m).contains("Ctrl+Alt+S"),
        "the hotkey is told"
    );
}

#[test]
fn the_texts_wrap_to_the_panel_so_a_narrower_one_is_taller() {
    use super::recording::body_height;
    let mut m = model();
    m.replay.state = ReplayState::Recording;
    m.replay.buffered_seconds = 12.0;
    m.replay.buffered_mb = 80.0;
    let mut ui = Ui::new();
    ui.begin(WINDOW, 1.0);
    let wide = body_height(&mut ui, &m, PANEL_WIDTH);
    let narrow = body_height(&mut ui, &m, 200.0);
    assert!(narrow > wide, "{narrow} {wide}");
}

#[test]
fn the_selected_quality_follows_the_bitrate() {
    let mut rig = recording_open();
    rig.model.replay.bitrate_mbps = 40;
    rig.frame(0.016);
    assert_eq!(rig.panel.slide["recording-quality"], 1.0);
    rig.model.replay.bitrate_mbps = 16;
    rig.frame(0.016);
    assert_eq!(rig.panel.slide["recording-quality"], 0.0);
}

#[test]
fn the_recording_body_stays_inside_the_panel_with_a_long_folder_path() {
    let mut rig = recording_open();
    rig.model.replay.folder = r"C:\Users\someone\Videos\".to_owned() + &"deeply nested ".repeat(12);
    rig.frame(0.016);
    let area = rig.panel.area().unwrap();
    assert!(rig
        .ui
        .vertices()
        .iter()
        .all(|v| v.pos[0] <= area.x + area.w + 4.0));
}

#[test]
fn the_section_height_is_the_one_the_module_computes() {
    use super::recording::body_height;
    let m = model();
    let mut ui = Ui::new();
    ui.begin(WINDOW, 1.0);
    assert_eq!(
        section_height(StudioSection::Recording, &m),
        body_height(&mut ui, &m, PANEL_WIDTH)
    );
}

#[test]
fn a_folded_section_takes_no_room_and_costs_no_measuring() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    let folded = rig
        .panel
        .hits
        .iter()
        .filter(|(h, _)| matches!(h, PanelHit::Header(_)))
        .count();
    assert_eq!(folded, 6);
    let (_, content_h) = stack(Scope::Full, &rig.panel.amount, &[0.0; 6]);
    assert_eq!(content_h, HEADER_H * 6.0, "only the six headers");
}
