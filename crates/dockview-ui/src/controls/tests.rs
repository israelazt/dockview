//! Tests of the live controls.

use super::buttons::{clock, record_label};
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
fn every_control_has_a_tooltip_in_both_languages_with_nothing_left_to_fill_in() {
    let hits = [
        Hit::ReplaySave,
        Hit::Record,
        Hit::Capture,
        Hit::Still,
        Hit::Mic,
        Hit::Hud,
        Hit::Fullscreen,
        Hit::Volume,
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
    let (x, y) = rig.controls.rect_of(Hit::Capture).center();
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
fn the_record_button_blinks_while_starting_and_is_steady_while_recording() {
    // The square of the button: 11 px (half 5.5) with a 3 px corner.
    let alpha_of_square = |rig: &Rig| {
        rig.ui
            .vertices()
            .iter()
            .find(|v| v.shape[3] == 0.0 && v.shape[2] == 3.0 && (v.shape[0] - 5.5).abs() < 0.6)
            .map(|v| v.color[3])
    };
    let mut rig = Rig::new(true);
    rig.model.rec = RecView {
        state: RecState::Starting,
        seconds: 0.0,
    };
    let mut seen = Vec::new();
    for _ in 0..40 {
        rig.frame(0.05);
        seen.push(alpha_of_square(&rig).unwrap());
    }
    let (lo, hi) = seen
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), a| (lo.min(*a), hi.max(*a)));
    assert!(hi - lo > 0.3, "blink range {lo}..{hi}");

    rig.model.rec = RecView {
        state: RecState::Recording,
        seconds: 3.0,
    };
    rig.frame(0.05);
    assert_eq!(alpha_of_square(&rig), Some(1.0));
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

// -- The bar's own buttons -------------------------------------------------

#[test]
fn the_capture_the_microphone_and_the_readouts_buttons_emit_their_actions() {
    for (hit, action) in [
        (Hit::Capture, Action::Capture),
        (Hit::Still, Action::TakeStill),
        (Hit::Mic, Action::ToggleMic),
        (Hit::Hud, Action::ToggleHud),
    ] {
        let mut rig = Rig::new(true);
        rig.click_on(hit);
        assert_eq!(rig.actions(), vec![action], "{hit:?}");
    }
}

#[test]
fn the_buttons_follow_the_state_of_the_microphone_and_the_readouts() {
    let mut m = model();
    m.mic.state = MicState::Live;
    assert!(tooltip_for(Hit::Mic, &m).unwrap().contains("off"));
    m.mic.state = MicState::Off;
    assert!(tooltip_for(Hit::Mic, &m).unwrap().contains("on"));
    m.hud = true;
    assert!(tooltip_for(Hit::Hud, &m).unwrap().starts_with("Hide"));
    m.hud = false;
    assert!(tooltip_for(Hit::Hud, &m).unwrap().starts_with("Show"));
}

#[test]
fn the_bar_has_the_clip_first_and_full_screen_last_inside_the_bar() {
    let rig = Rig::new(true);
    let save = rig.controls.rect_of(Hit::ReplaySave);
    let record = rig.controls.rect_of(Hit::Record);
    let capture = rig.controls.rect_of(Hit::Capture);
    let volume = rig.controls.rect_of(Hit::Volume);
    let (mic, hud, full) = (
        rig.controls.rect_of(Hit::Mic),
        rig.controls.rect_of(Hit::Hud),
        rig.controls.rect_of(Hit::Fullscreen),
    );
    assert!(save.x < record.x && record.x < capture.x && capture.x < volume.x);
    let still = rig.controls.rect_of(Hit::Still);
    assert!(volume.x < still.x && still.x < mic.x);
    assert!(volume.x < mic.x && mic.x < hud.x && hud.x < full.x);
    let pill = rig.controls.rect_of(Hit::Pill);
    assert!(save.x >= pill.x && full.x + full.w <= pill.x + pill.w);
    assert!(pill.x >= 0.0 && pill.x + pill.w <= 1280.0);
}

#[test]
fn the_record_button_shows_the_time_while_recording() {
    assert_eq!(clock(0.0), "0:00");
    assert_eq!(clock(65.0), "1:05");
    assert_eq!(clock(3725.0), "1:02:05");
    let mut m = model();
    m.lang = Lang::En;
    assert_eq!(record_label(&m), "Record");
    m.rec = RecView {
        state: RecState::Recording,
        seconds: 42.0,
    };
    assert_eq!(record_label(&m), "0:42");
}

#[test]
fn a_bar_that_does_not_fit_the_video_area_drops_the_hotkeys_and_shrinks_the_buttons() {
    let measure = |limit: f32| {
        let mut m = model();
        m.replay = ReplayView {
            record_hotkey: "Ctrl+Alt+R".into(),
            ..ReplayView::off("Ctrl+Alt+S")
        };
        let mut rig = Rig::new(true);
        rig.model = m;
        rig.limit = Some(limit);
        rig.frame(0.016);
        let pill = rig.controls.rect_of(Hit::Pill);
        (pill, rig.controls.rect_of(Hit::ReplaySave))
    };
    let (full, save_full) = measure(1280.0);
    // Room for less: the hotkeys go and the buttons shrink.
    let (short, save_short) = measure(full.w - 20.0);
    assert!(short.w < full.w && save_short.w < save_full.w);
    assert!(
        short.x >= 8.0 && short.x + short.w <= full.w - 20.0,
        "{short:?} in {}",
        full.w - 20.0
    );
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

#[test]
fn the_pointer_is_over_the_controls_on_the_pill_and_its_buttons_only() {
    let mut rig = Rig::new(true);
    assert!(!rig.controls.over_controls());
    let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x, y);
    assert!(rig.controls.over_controls(), "on a button");
    let pill = rig.controls.rect_of(Hit::Pill);
    rig.move_to(pill.x + 4.0, pill.y + pill.h / 2.0);
    assert!(rig.controls.over_controls(), "on the pill's background");
    rig.move_to(20.0, 20.0);
    assert!(!rig.controls.over_controls(), "on the picture");
}

#[test]
fn with_the_screenshot_tool_off_the_pill_has_no_capture_button_and_is_narrower() {
    let mut on = Rig::new(true);
    on.frame(0.016);
    let with = on.controls.rect_of(Hit::Pill).w;
    assert!(on.controls.hits.iter().any(|(h, _)| *h == Hit::Capture));

    let mut off = Rig::new(true);
    off.model.snip = false;
    off.frame(0.016);
    assert!(off.controls.hits.iter().all(|(h, _)| *h != Hit::Capture));
    assert!(off.controls.rect_of(Hit::Pill).w < with);
}

#[test]
fn hiding_the_controls_takes_them_off_at_once_and_leaves_nothing_under_the_pointer() {
    let mut rig = Rig::new(true);
    let (x, y) = rig.controls.rect_of(Hit::Fullscreen).center();
    rig.move_to(x, y);
    assert!(rig.controls.over_controls() && rig.controls.wants_pointer());
    rig.controls.hide();
    assert!(!rig.controls.is_visible());
    assert!(!rig.controls.over_controls() && !rig.controls.wants_pointer());
    // A press now reaches nothing: there is nothing to press.
    rig.pointer.press();
    rig.controls.hide();
    assert!(rig.actions().is_empty());
}

// -- The length of the clip, beside the save button ---------------------------

#[test]
fn the_length_of_a_clip_is_written_short() {
    use super::pill::length_label;
    assert_eq!(length_label(30), "30 s");
    assert_eq!(length_label(120), "2 min");
    assert_eq!(length_label(90), "1:30");
    for step in dockview_domain::settings::REPLAY_STEPS {
        assert!(length_label(step).chars().count() <= 6, "{step}");
    }
}

#[test]
fn the_chip_sits_between_the_save_and_record_buttons_and_opens_the_selector() {
    let mut rig = Rig::new(true);
    let save = rig.controls.rect_of(Hit::ReplaySave);
    let chip = rig.controls.rect_of(Hit::ReplayLength);
    let record = rig.controls.rect_of(Hit::Record);
    assert!(save.x + save.w <= chip.x && chip.x + chip.w <= record.x);
    assert!(!rig.controls.length_open);
    rig.click_on(Hit::ReplayLength);
    assert!(rig.controls.length_open);
    rig.frame(0.016);
    // The selector is over the bar, not over the buttons.
    let pill = rig.controls.rect_of(Hit::Pill);
    let popover = rig.controls.rect_of(Hit::LengthPopover);
    assert!(popover.y + popover.h <= pill.y);
    // A second click on the chip closes it.
    rig.click_on(Hit::ReplayLength);
    assert!(!rig.controls.length_open);
}

#[test]
fn the_selector_buttons_ask_for_a_step_shorter_or_longer_and_stay_open() {
    let mut rig = Rig::new(true);
    rig.click_on(Hit::ReplayLength);
    rig.frame(0.016);
    rig.click_on(Hit::LengthMore);
    assert_eq!(rig.actions(), vec![Action::ReplayLength(1)]);
    rig.frame(0.016);
    rig.click_on(Hit::LengthLess);
    assert_eq!(rig.actions(), vec![Action::ReplayLength(-1)]);
    assert!(rig.controls.length_open, "it stays for another step");
}

#[test]
fn a_press_anywhere_else_closes_the_selector_and_asks_for_nothing() {
    let mut rig = Rig::new(true);
    rig.click_on(Hit::ReplayLength);
    rig.frame(0.016);
    rig.click(20.0, 20.0);
    assert!(!rig.controls.length_open);
    assert!(rig.actions().is_empty());
}

#[test]
fn hiding_the_controls_closes_the_selector() {
    let mut rig = Rig::new(true);
    rig.click_on(Hit::ReplayLength);
    rig.controls.hide();
    assert!(!rig.controls.length_open);
}

#[test]
fn the_bar_does_not_change_its_width_when_the_length_does() {
    let mut rig = Rig::new(true);
    let before = rig.controls.rect_of(Hit::Pill);
    for seconds in [5, 90, 900] {
        rig.model.replay.keep_seconds = seconds;
        rig.frame(0.016);
        assert_eq!(rig.controls.rect_of(Hit::Pill), before, "{seconds} s");
    }
}

#[test]
fn the_chip_has_a_tooltip_that_says_the_length() {
    let mut m = model();
    m.replay.keep_seconds = 120;
    let tip = tooltip_for(Hit::ReplayLength, &m).expect("a tooltip");
    assert!(tip.contains("2 min"), "{tip}");
    m.lang = Lang::Es;
    let tip = tooltip_for(Hit::ReplayLength, &m).expect("a tooltip");
    assert!(tip.contains("Duración"), "{tip}");
}
