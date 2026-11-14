//! Tests of the live controls.

use super::tooltip::*;
use super::*;
use crate::motion::{ease_out_cubic, lerp_color, move_toward};
use crate::test_model::model;
use dockview_domain::i18n::Lang;
use dockview_domain::theme::Color;

pub(super) struct Rig {
    ui: Ui,
    pub(super) controls: Controls,
    pub(super) model: Model,
    pointer: Pointer,
    /// The width left for the video, when something else takes the rest.
    limit: Option<f32>,
}

impl Rig {
    pub(super) fn new(always_visible: bool) -> Self {
        let mut rig = Self {
            ui: Ui::new(),
            controls: Controls::new(always_visible),
            model: model(),
            pointer: Pointer::default(),
            limit: None,
        };
        rig.frame(0.016);
        rig
    }

    pub(super) fn frame(&mut self, dt: f32) {
        self.ui.begin((1280, 720), 1.0);
        self.ui.limit_width(self.limit);
        self.controls
            .frame(&mut self.ui, &self.model, &self.pointer, dt);
        self.pointer.end_frame();
    }

    pub(super) fn move_to(&mut self, x: f32, y: f32) {
        self.pointer.pos = Some((x, y));
        self.frame(0.016);
    }

    pub(super) fn click(&mut self, x: f32, y: f32) {
        self.move_to(x, y);
        self.pointer.press();
        self.frame(0.016);
        self.pointer.release();
        self.frame(0.016);
    }

    pub(super) fn click_on(&mut self, hit: Hit) {
        let (x, y) = self.controls.rect_of(hit).center();
        self.click(x, y);
    }

    pub(super) fn actions(&mut self) -> Vec<Action> {
        self.controls.take_actions()
    }
}

#[test]
fn controls_fade_out_after_idle_and_draw_nothing() {
    let mut rig = Rig::new(false);
    rig.move_to(10.0, 10.0); // wake up
    for _ in 0..20 {
        rig.frame(0.1);
    }
    assert!(rig.controls.is_visible(), "still inside the idle window");
    for _ in 0..40 {
        rig.frame(0.1);
    }
    assert!(!rig.controls.is_visible());
    assert!(rig.ui.vertices().is_empty());
}

#[test]
fn moving_the_pointer_wakes_hidden_controls() {
    let mut rig = Rig::new(false);
    for _ in 0..60 {
        rig.frame(0.1);
    }
    assert!(!rig.controls.is_visible());
    rig.move_to(300.0, 300.0);
    rig.move_to(310.0, 300.0);
    for _ in 0..20 {
        rig.frame(0.016);
    }
    assert!(rig.controls.is_visible());
}

#[test]
fn preview_mode_never_hides() {
    let mut rig = Rig::new(true);
    for _ in 0..100 {
        rig.frame(0.1);
    }
    assert!(rig.controls.is_visible());
}

#[test]
fn fade_steps_take_the_configured_time() {
    let mut v = 0.0;
    for _ in 0..10 {
        v = step_visibility(v, true, FADE_SECS / 10.0);
    }
    assert!((v - 1.0).abs() < 1e-4);
    assert_eq!(step_visibility(0.1, false, 1.0), 0.0);
}

#[test]
fn dragging_the_slider_sets_the_volume_from_the_pointer() {
    let mut rig = Rig::new(true);
    let track = rig.controls.volume_track;
    rig.move_to(track.x + track.w * 0.25, track.y);
    rig.pointer.press();
    rig.frame(0.016);
    rig.move_to(track.x + track.w * 0.75, track.y);
    rig.pointer.release();
    rig.frame(0.016);

    let volumes: Vec<f32> = rig
        .actions()
        .into_iter()
        .filter_map(|a| {
            if let Action::SetVolume(v) = a {
                Some(v)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(volumes.first().copied(), Some(0.25));
    assert_eq!(volumes.last().copied(), Some(0.75));
}

#[test]
fn dragging_past_the_ends_clamps_the_volume() {
    let mut rig = Rig::new(true);
    let track = rig.controls.volume_track;
    rig.move_to(track.x + 10.0, track.y);
    rig.pointer.press();
    rig.frame(0.016);
    rig.move_to(track.x + track.w + 200.0, track.y);
    let last = rig.actions().into_iter().rev().find_map(|a| match a {
        Action::SetVolume(v) => Some(v),
        _ => None,
    });
    assert_eq!(last, Some(1.0));
}

#[test]
fn scrolling_over_the_pill_changes_the_volume() {
    let mut rig = Rig::new(true);
    let pill = rig.controls.rect_of(Hit::Pill);
    rig.move_to(pill.x + 30.0, pill.y + 28.0);
    rig.pointer.wheel = 1.0;
    rig.frame(0.016);
    assert_eq!(rig.actions(), vec![Action::SetVolume(0.55)]);
}

#[test]
fn scrolling_away_from_the_controls_does_nothing() {
    let mut rig = Rig::new(true);
    rig.move_to(20.0, 20.0);
    rig.pointer.wheel = 1.0;
    rig.frame(0.016);
    assert!(rig.actions().is_empty());
}

#[test]
fn sync_buttons_step_by_ten_and_clamp() {
    let mut rig = Rig::new(true);
    rig.click_on(Hit::SyncPlus);
    assert_eq!(rig.actions(), vec![Action::SetSync(10)]);
    rig.click_on(Hit::SyncMinus);
    assert_eq!(rig.actions(), vec![Action::SetSync(-10)]);

    rig.model.sync_ms = MAX_AUDIO_OFFSET_MS;
    rig.frame(0.016);
    rig.click_on(Hit::SyncPlus);
    assert_eq!(rig.actions(), vec![Action::SetSync(MAX_AUDIO_OFFSET_MS)]);
}

#[test]
fn every_control_has_a_tooltip_in_both_languages_with_nothing_left_to_fill_in() {
    let hits = [
        Hit::Sources,
        Hit::Scenes,
        Hit::Studio,
        Hit::Replay,
        Hit::ReplaySave,
        Hit::Record,
        Hit::Settings,
        Hit::Fullscreen,
        Hit::Volume,
        Hit::SyncMinus,
        Hit::SyncPlus,
    ];
    for lang in [Lang::En, Lang::Es] {
        let mut m = model();
        m.lang = lang;
        m.replay = ReplayView {
            record_hotkey: "Ctrl+Alt+R".into(),
            ..ReplayView::off("Ctrl+Alt+S")
        };
        for hit in hits {
            let text = tooltip_for(hit, &m).unwrap_or_else(|| panic!("{hit:?} has no tooltip"));
            assert!(
                !text.trim().is_empty() && !text.contains(['{', '}']),
                "{lang:?} {hit:?}: {text}"
            );
        }
    }
    // The background says nothing.
    let m = model();
    assert_eq!(tooltip_for(Hit::Pill, &m), None);
}

#[test]
fn tooltips_name_the_hotkey_and_follow_the_state() {
    let mut m = model();
    m.replay = ReplayView {
        state: ReplayState::Recording,
        keep_seconds: 30,
        record_hotkey: "Ctrl+Alt+R".into(),
        ..ReplayView::off("Ctrl+Alt+S")
    };
    let save = tooltip_for(Hit::ReplaySave, &m).unwrap();
    assert!(
        save.contains("30 s") && save.contains("Ctrl+Alt+S"),
        "{save}"
    );
    assert!(tooltip_for(Hit::Record, &m)
        .unwrap()
        .contains("Start recording (Ctrl+Alt+R)"));
    m.rec.state = RecState::Recording;
    assert!(tooltip_for(Hit::Record, &m)
        .unwrap()
        .contains("Stop recording"));
    m.fullscreen = true;
    assert!(tooltip_for(Hit::Fullscreen, &m)
        .unwrap()
        .contains("Leave fullscreen"));
    m.replay.state = ReplayState::Off;
    assert!(tooltip_for(Hit::ReplaySave, &m).unwrap().contains("off"));
}

#[test]
fn a_tooltip_appears_after_resting_on_a_button_and_not_before() {
    let mut rig = Rig::new(true);
    let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x, y);
    let before = rig.ui.vertices().len();
    rig.frame(0.2);
    assert_eq!(rig.ui.vertices().len(), before, "too early for a tooltip");
    rig.frame(0.4); // 0.6 s resting
    rig.frame(0.2);
    assert!(rig.ui.vertices().len() > before, "the tooltip is drawn");
}

#[test]
fn pressing_hides_the_tooltip_and_moving_away_resets_the_wait() {
    let mut rig = Rig::new(true);
    let (x, y) = rig.controls.rect_of(Hit::Settings).center();
    rig.move_to(x, y);
    rig.frame(0.6);
    rig.frame(0.1);
    let shown = rig.ui.vertices().len();
    rig.pointer.press();
    rig.frame(0.1);
    assert!(rig.ui.vertices().len() < shown, "no tooltip while pressed");
    rig.pointer.release();
    // Another button: the wait starts again.
    let (x2, y2) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x2, y2);
    rig.frame(0.1);
    let early = rig.ui.vertices().len();
    rig.frame(0.5);
    rig.frame(0.1);
    assert!(rig.ui.vertices().len() > early);
}

#[test]
fn a_tooltip_stays_inside_the_window_even_on_the_last_button() {
    for lang in [Lang::En, Lang::Es] {
        let mut rig = Rig::new(true);
        rig.model.lang = lang;
        rig.frame(0.016);
        let button = rig.controls.rect_of(Hit::Fullscreen);
        let (x, y) = button.center();
        rig.move_to(x, y);
        rig.frame(0.7);
        rig.frame(0.2);
        let max_x = rig
            .ui
            .vertices()
            .iter()
            .map(|v| v.pos[0])
            .fold(0.0f32, f32::max);
        assert!(max_x <= 1280.0 + 2.0, "{lang:?}: drawn up to x={max_x}");
    }
}

#[test]
fn the_record_button_emits_its_action_in_every_state() {
    for state in [RecState::Idle, RecState::Starting, RecState::Recording] {
        let mut rig = Rig::new(true);
        rig.model.rec = RecView {
            state,
            seconds: 42.0,
        };
        rig.frame(0.016);
        rig.click_on(Hit::Record);
        assert_eq!(rig.actions(), vec![Action::ToggleRecord], "{state:?}");
        assert_eq!(rig.model.rec.is_on(), state != RecState::Idle);
    }
}

#[test]
fn the_pill_save_button_emits_a_save_even_when_the_buffer_is_off() {
    // The app answers with "turn on instant replay first".
    let mut rig = Rig::new(true);
    rig.click_on(Hit::ReplaySave);
    assert_eq!(rig.actions(), vec![Action::SaveReplay]);
}

#[test]
fn the_pill_still_fits_with_the_replay_buttons_in_both_languages() {
    for lang in [Lang::En, Lang::Es] {
        let mut rig = Rig::new(true);
        rig.model.lang = lang;
        rig.frame(0.016);
        let pill = rig.controls.rect_of(Hit::Pill);
        assert!(
            pill.x >= 0.0 && pill.x + pill.w <= 1280.0,
            "{lang:?}: {pill:?}"
        );
    }
}

#[test]
fn colours_blend_linearly_and_clamp() {
    let (a, b) = (Color::hex(0x000000, 0.0), Color::hex(0xFFFFFF, 1.0));
    let mid = lerp_color(a, b, 0.5);
    assert!((mid.r - 0.5).abs() < 1e-6 && (mid.a - 0.5).abs() < 1e-6);
    assert_eq!(lerp_color(a, b, -3.0), a);
    assert_eq!(lerp_color(a, b, 9.0), b);
}

#[test]
fn fullscreen_button_emits_an_action() {
    let mut rig = Rig::new(true);
    rig.click_on(Hit::Fullscreen);
    assert_eq!(rig.actions(), vec![Action::ToggleFullscreen]);
}

#[test]
fn releasing_elsewhere_cancels_a_click() {
    let mut rig = Rig::new(true);
    let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x, y);
    rig.pointer.press();
    rig.frame(0.016);
    rig.move_to(30.0, 30.0);
    rig.pointer.release();
    rig.frame(0.016);
    assert!(rig.actions().is_empty());
}

#[test]
fn pill_width_does_not_change_when_numbers_change() {
    let mut rig = Rig::new(true);
    let before = rig.controls.rect_of(Hit::Pill);
    rig.model.volume = 1.0;
    rig.model.sync_ms = -150;
    rig.model.resolution = "720p · 30 fps".into();
    rig.frame(0.016);
    assert_eq!(rig.controls.rect_of(Hit::Pill), before);
}

#[test]
fn pill_fits_a_small_window_in_both_languages() {
    for lang in [Lang::En, Lang::Es] {
        let mut rig = Rig::new(true);
        rig.model.lang = lang;
        rig.frame(0.016);
        let pill = rig.controls.rect_of(Hit::Pill);
        assert!(pill.x >= 0.0 && pill.x + pill.w <= 1280.0, "{lang:?}");
    }
}

#[test]
fn spanish_pill_is_wider_than_english() {
    let mut en = Rig::new(true);
    let mut es = Rig::new(true);
    es.model.lang = Lang::Es;
    es.frame(0.016);
    en.frame(0.016);
    assert!(es.controls.rect_of(Hit::Pill).w > en.controls.rect_of(Hit::Pill).w);
}

#[test]
fn smoothing_converges_monotonically_and_snaps() {
    let mut v = 0.0;
    let mut last = 0.0;
    for _ in 0..120 {
        v = smooth(v, 1.0, 0.016, HOVER_RATE);
        assert!(v >= last && v <= 1.0);
        last = v;
    }
    assert_eq!(v, 1.0, "close values snap exactly onto the target");
    assert!(
        smooth(0.0, 1.0, 0.016, HOVER_RATE) < 0.5,
        "one frame is a small step"
    );
}

#[test]
fn easing_endpoints_and_direction() {
    assert_eq!(ease_out_cubic(0.0), 0.0);
    assert_eq!(ease_out_cubic(1.0), 1.0);
    assert!(ease_out_cubic(0.5) > 0.5, "ease-out leads a linear ramp");
    assert_eq!(move_toward(0.9, 1.0, 0.5), 1.0);
    assert_eq!(move_toward(0.2, 0.0, 0.5), 0.0);
}

#[test]
fn hover_fades_in_and_out_instead_of_jumping() {
    let mut rig = Rig::new(true);
    let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x, y);
    rig.frame(0.016);
    let early = rig.controls.amounts[&Hit::Fullscreen].0;
    assert!(early > 0.0 && early < 1.0, "mid-fade, got {early}");
    for _ in 0..30 {
        rig.frame(0.016);
    }
    assert_eq!(rig.controls.amounts[&Hit::Fullscreen].0, 1.0);

    rig.move_to(20.0, 20.0);
    rig.frame(0.016);
    let leaving = rig.controls.amounts[&Hit::Fullscreen].0;
    assert!(leaving > 0.0 && leaving < 1.0, "fading out, got {leaving}");
}

#[test]
fn pressing_a_button_dips_it() {
    let mut rig = Rig::new(true);
    let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x, y);
    rig.pointer.press();
    for _ in 0..10 {
        rig.frame(0.016);
    }
    assert!(rig.controls.amounts[&Hit::Fullscreen].1 > 0.5);
}

#[test]
fn disabling_animations_makes_everything_instant() {
    let mut rig = Rig::new(true);
    rig.controls.set_animations(false);
    let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x, y);
    assert_eq!(rig.controls.amounts[&Hit::Fullscreen].0, 1.0);
}

#[test]
fn connecting_status_pulses_but_live_does_not() {
    let alpha_of_dot = |rig: &Rig| {
        rig.ui
            .vertices()
            .iter()
            .find(|v| v.shape[3] == 0.0 && v.shape[0] == 4.0)
            .map(|v| v.color[3])
    };
    let mut rig = Rig::new(true);
    rig.model.status = StatusKind::Connecting;
    let mut seen = Vec::new();
    for _ in 0..40 {
        rig.frame(0.05);
        seen.push(alpha_of_dot(&rig).unwrap());
    }
    let (lo, hi) = seen
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), a| (lo.min(*a), hi.max(*a)));
    assert!(hi - lo > 0.3, "pulse range {lo}..{hi}");

    rig.model.status = StatusKind::Live;
    rig.frame(0.05);
    assert_eq!(alpha_of_dot(&rig), Some(1.0));
}

#[test]
fn wants_pointer_only_over_widgets_not_backgrounds() {
    let mut rig = Rig::new(true);
    let pill = rig.controls.rect_of(Hit::Pill);
    rig.move_to(pill.x + 8.0, pill.y + 2.0); // pill background corner
    assert!(!rig.controls.wants_pointer());
    let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x, y);
    assert!(rig.controls.wants_pointer());
}

// -- The Studio button ----------------------------------------------------

#[test]
fn the_studio_button_asks_to_toggle_the_panel() {
    let mut rig = Rig::new(true);
    rig.click_on(Hit::Studio);
    assert_eq!(rig.actions(), [Action::ToggleStudio]);
}

#[test]
fn the_studio_button_says_open_or_close_by_what_it_would_do() {
    let mut m = model();
    let open = tooltip_for(Hit::Studio, &m).expect("a tooltip");
    assert!(open.contains("Tab"), "the key is in the tooltip: {open}");
    m.studio = true;
    let close = tooltip_for(Hit::Studio, &m).expect("a tooltip");
    assert_ne!(open, close);
    assert!(close.starts_with("Close"), "{close}");
    // Full screen hides the panel, so the button offers to open it again.
    m.fullscreen = true;
    assert_eq!(tooltip_for(Hit::Studio, &m).unwrap(), open);
}

#[test]
fn the_studio_button_sits_between_settings_and_full_screen_and_fits_the_pill() {
    let rig = Rig::new(true);
    let (settings, studio, fullscreen) = (
        rig.controls.rect_of(Hit::Settings),
        rig.controls.rect_of(Hit::Studio),
        rig.controls.rect_of(Hit::Fullscreen),
    );
    assert!(settings.x < studio.x && studio.x < fullscreen.x);
    let pill = rig.controls.rect_of(Hit::Pill);
    assert!(
        fullscreen.x + fullscreen.w <= pill.x + pill.w,
        "inside the pill"
    );
    assert!(
        pill.x >= 0.0 && pill.x + pill.w <= 1280.0,
        "inside a 1280 window"
    );
}

#[test]
fn a_pill_that_does_not_fit_the_video_area_drops_the_resolution_then_the_sync() {
    let width_in = |limit: f32| {
        let mut rig = Rig::new(true);
        rig.limit = Some(limit);
        rig.frame(0.016);
        let pill = rig.controls.rect_of(Hit::Pill);
        (
            pill,
            rig.controls.hits.iter().any(|(h, _)| *h == Hit::SyncPlus),
        )
    };
    let (full, has_sync) = width_in(1280.0);
    assert!(has_sync);
    // Room for everything but the resolution.
    let (short, has_sync) = width_in(full.w - 20.0);
    assert!(has_sync && short.w < full.w);
    // Room for less still: no sync block either.
    let (least, has_sync) = width_in(short.w - 20.0);
    assert!(!has_sync && least.w < short.w);
    for (pill, limit) in [(short, full.w - 20.0), (least, short.w - 20.0)] {
        assert!(
            pill.x >= 8.0 && pill.x + pill.w <= limit,
            "{pill:?} in {limit}"
        );
    }
}

#[test]
fn the_pill_is_centred_in_the_video_area_not_in_the_window() {
    let mut rig = Rig::new(true);
    rig.limit = Some(960.0);
    rig.frame(0.016);
    let pill = rig.controls.rect_of(Hit::Pill);
    let middle = pill.x + pill.w / 2.0;
    assert!((middle - 480.0).abs() <= 1.0, "{middle}");
}

// -- Buttons that open the Studio panel ----------------------------------

#[test]
fn pill_buttons_ask_for_the_panel_on_their_section() {
    use crate::studio::StudioSection as S;
    let cases = [
        (Hit::Settings, S::Settings),
        (Hit::Replay, S::Recording),
        (Hit::Sources, S::Sources),
        (Hit::Scenes, S::Scenes),
    ];
    for (hit, section) in cases {
        let mut rig = Rig::new(true);
        rig.click_on(hit);
        assert_eq!(rig.actions(), [Action::OpenStudio], "{hit:?}");
        assert_eq!(rig.controls.take_studio_request(), Some(section), "{hit:?}");
        assert_eq!(rig.controls.take_studio_request(), None, "asked once");
    }
}
