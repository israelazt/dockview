//! Tests of the Studio panel.

use dockview_domain::layout::{studio_layout, PANEL_WIDTH};
use dockview_domain::view::{Action, Model};

use super::sections::{StudioSection, HEADER_H};
use super::*;
use crate::test_model::model;

const WINDOW: (u32, u32) = (1280, 720);

/// A panel driven the way the window drives it: it opens, the layout gives it
/// its room, and it draws into it.
struct Rig {
    ui: Ui,
    panel: StudioPanel,
    model: Model,
    pointer: Pointer,
    window: (u32, u32),
    visible: bool,
}

impl Rig {
    fn new() -> Self {
        let mut panel = StudioPanel::new();
        panel.set_animations(false);
        Self {
            ui: Ui::new(),
            panel,
            model: model(),
            pointer: Pointer::default(),
            window: WINDOW,
            visible: true,
        }
    }

    fn frame(&mut self, dt: f32) {
        self.ui.begin(self.window, 1.0);
        self.panel.update(self.visible, false, dt);
        let layout = studio_layout(self.window, 1.0, self.panel.openness());
        let area = layout.panel.map(|r| Rect::new(r.x, r.y, r.w, r.h));
        let full = studio_layout(self.window, 1.0, 1.0).panel.unwrap().w;
        self.panel
            .frame(&mut self.ui, &self.model, area, full, &self.pointer, dt);
        self.pointer.end_frame();
    }

    fn move_to(&mut self, x: f32, y: f32) {
        self.pointer.pos = Some((x, y));
        self.frame(0.016);
    }

    fn click(&mut self, x: f32, y: f32) {
        self.move_to(x, y);
        self.pointer.press();
        self.frame(0.016);
        self.pointer.release();
        self.frame(0.016);
        self.frame(0.016); // the click's effect is drawn and settles
    }

    fn rect_of(&self, hit: PanelHit) -> Rect {
        self.panel
            .hits
            .iter()
            .rev()
            .find(|(h, _)| *h == hit)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{hit:?} was not drawn"))
    }

    fn click_on(&mut self, hit: PanelHit) {
        let (x, y) = self.rect_of(hit).center();
        self.click(x, y);
    }

    /// Opens every section, last first (opening one pushes the ones below it
    /// out of the window).
    fn open_all(&mut self) {
        for section in StudioSection::ALL.into_iter().rev() {
            self.click_on(PanelHit::Header(section));
        }
        self.frame(0.016);
    }

    fn wheel(&mut self, notches: f32) {
        self.pointer.pos = Some((1100.0, 200.0));
        self.pointer.wheel = notches;
        self.frame(0.016);
    }

    /// Vertices well above the list (where only the panel's own header is;
    /// the margin leaves out the soft edge of the scroll bar's thumb).
    fn vertices_above_the_list(&self) -> usize {
        self.ui
            .vertices()
            .iter()
            .filter(|v| v.pos[1] < TOP_H - 4.0)
            .count()
    }
}

#[test]
fn a_closed_panel_draws_nothing_and_takes_no_room() {
    let mut rig = Rig::new();
    rig.visible = false;
    rig.frame(0.016);
    assert!(rig.ui.vertices().is_empty());
    assert_eq!(rig.panel.area(), None);
    assert!(!rig.panel.wants_pointer());
}

#[test]
fn an_open_panel_fills_the_right_of_the_window_at_its_width() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    let area = rig.panel.area().expect("open");
    assert_eq!(area.w, PANEL_WIDTH);
    assert_eq!(area.x + area.w, WINDOW.0 as f32);
    assert_eq!(area.h, WINDOW.1 as f32);
    assert!(!rig.ui.vertices().is_empty());
}

#[test]
fn it_opens_and_closes_over_the_animation_time_with_a_fast_start() {
    let mut rig = Rig::new();
    rig.panel.set_animations(true);
    rig.frame(0.016);
    let early = rig.panel.openness();
    assert!(early > 0.0 && early < 1.0);
    assert!(rig.panel.is_animating());
    // A fast start: a tenth of the time already opens a good part.
    let mut quick = StudioPanel::new();
    quick.update(true, false, OPEN_SECS / 10.0);
    assert!(quick.openness() > 0.2, "{}", quick.openness());

    let mut last = early;
    for _ in 0..30 {
        rig.frame(0.016);
        assert!(
            rig.panel.openness() >= last,
            "never goes back while opening"
        );
        last = rig.panel.openness();
    }
    assert_eq!(rig.panel.openness(), 1.0);

    rig.visible = false;
    for _ in 0..30 {
        rig.frame(0.016);
    }
    assert_eq!(rig.panel.openness(), 0.0);
    assert_eq!(rig.panel.area(), None);
}

#[test]
fn the_width_follows_the_opening() {
    let mut rig = Rig::new();
    rig.panel.set_animations(true);
    rig.frame(0.05);
    let half = rig.panel.area().expect("opening").w;
    assert!(half > 0.0 && half < PANEL_WIDTH, "{half}");
}

#[test]
fn without_animations_it_opens_and_closes_at_once() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    assert_eq!(rig.panel.openness(), 1.0);
    rig.visible = false;
    rig.frame(0.016);
    assert_eq!(rig.panel.openness(), 0.0);
    assert!(!rig.panel.is_animating());
}

#[test]
fn full_screen_closes_it_at_once_even_with_animations() {
    let mut panel = StudioPanel::new();
    panel.update(true, false, 1.0);
    assert_eq!(panel.openness(), 1.0);
    panel.update(false, true, 0.001);
    assert_eq!(
        panel.openness(),
        0.0,
        "no slide when the video takes the window"
    );
}

#[test]
fn a_settled_panel_asks_for_no_more_frames() {
    let mut rig = Rig::new();
    rig.panel.set_animations(true);
    for _ in 0..60 {
        rig.frame(0.016);
    }
    assert!(!rig.panel.is_animating());
}

#[test]
fn the_close_button_asks_to_toggle_the_studio() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    rig.click_on(PanelHit::Close);
    assert_eq!(rig.panel.take_actions(), [Action::ToggleStudio]);
    assert!(rig.panel.take_actions().is_empty());
}

#[test]
fn a_press_that_is_released_somewhere_else_does_nothing() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    let (x, y) = rig.rect_of(PanelHit::Close).center();
    rig.move_to(x, y);
    rig.pointer.press();
    rig.frame(0.016);
    rig.move_to(1000.0, 400.0); // away from the button
    rig.pointer.release();
    rig.frame(0.016);
    assert!(rig.panel.take_actions().is_empty());
}

#[test]
fn clicking_a_header_unfolds_the_section_and_clicking_again_folds_it() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    let first = StudioSection::Sources;
    assert!(!rig.panel.is_open(first));
    let closed_height = rig.rect_of(PanelHit::Header(StudioSection::Scenes)).y;

    rig.click_on(PanelHit::Header(first));
    assert!(rig.panel.is_open(first));
    let pushed = rig.rect_of(PanelHit::Header(StudioSection::Scenes)).y;
    assert!(pushed > closed_height, "the section below moves down");

    rig.click_on(PanelHit::Header(first));
    assert!(!rig.panel.is_open(first));
    assert_eq!(
        rig.rect_of(PanelHit::Header(StudioSection::Scenes)).y,
        closed_height
    );
}

#[test]
fn sections_open_and_fold_on_their_own() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    rig.click_on(PanelHit::Header(StudioSection::Audio));
    for section in StudioSection::ALL {
        assert_eq!(rig.panel.is_open(section), section == StudioSection::Audio);
    }
}

#[test]
fn unfolding_is_animated_when_animations_are_on() {
    let mut rig = Rig::new();
    rig.panel.set_animations(true);
    for _ in 0..30 {
        rig.frame(0.016);
    }
    rig.click_on(PanelHit::Header(StudioSection::Sources));
    let at_start = rig.panel.amount[0];
    assert!(at_start > 0.0 && at_start < 1.0, "{at_start}");
    assert!(rig.panel.is_animating());
    for _ in 0..60 {
        rig.frame(0.016);
    }
    assert_eq!(rig.panel.amount[0], 1.0);
}

/// A window too short for the six sections open at once.
fn tall_window(rig: &mut Rig) {
    rig.window = (1280, 400);
    rig.frame(0.016); // draws once at the new size, so there is something to click
    rig.open_all();
}

#[test]
fn the_wheel_scrolls_a_list_that_does_not_fit_and_stops_at_the_ends() {
    let mut rig = Rig::new();
    tall_window(&mut rig);
    // Audio is the third section: it stays on show while the list scrolls.
    let audio = PanelHit::Header(StudioSection::Audio);
    let before = rig.rect_of(audio).y;

    rig.wheel(-1.0);
    assert_eq!(before - rig.rect_of(audio).y, scroll::NOTCH_PX);

    rig.wheel(1.0);
    assert_eq!(rig.rect_of(audio).y, before);
    rig.wheel(5.0);
    assert_eq!(rig.rect_of(audio).y, before, "not above the top");

    rig.wheel(-100.0);
    let (_, content_h) = stack(&rig.panel.amount);
    let list_h = 400.0 - TOP_H;
    assert_eq!(rig.panel.scroll, content_h - list_h, "not past the end");
}

#[test]
fn a_list_that_fits_does_not_scroll() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    rig.wheel(-3.0);
    assert_eq!(rig.panel.scroll, 0.0);
}

#[test]
fn folding_a_section_while_scrolled_to_the_end_pulls_the_scroll_back() {
    let mut rig = Rig::new();
    tall_window(&mut rig);
    rig.wheel(-100.0);
    let far = rig.panel.scroll;
    // Fold the last section (its header is on screen at the end of the list).
    rig.click_on(PanelHit::Header(StudioSection::Stats));
    rig.frame(0.016);
    assert!(rig.panel.scroll < far);
    let (_, content_h) = stack(&rig.panel.amount);
    assert!(rig.panel.scroll <= scroll::max_scroll(content_h, 400.0 - TOP_H));
}

#[test]
fn what_scrolls_out_of_the_list_is_cut_and_never_covers_the_panel_header() {
    let mut rig = Rig::new();
    tall_window(&mut rig);
    let at_the_top = rig.vertices_above_the_list();
    rig.wheel(-100.0);
    assert_eq!(
        rig.vertices_above_the_list(),
        at_the_top,
        "only the panel's own header is drawn above the list"
    );
    // The panel's own background has a soft edge of a couple of pixels; what
    // scrolled past the bottom of the list must not go beyond it.
    assert!(
        rig.ui.vertices().iter().all(|v| v.pos[1] <= 404.0),
        "and nothing below the window"
    );
}

#[test]
fn a_header_scrolled_under_the_top_cannot_be_clicked_where_it_is_cut() {
    let mut rig = Rig::new();
    tall_window(&mut rig);
    // Scrolled so that the second header straddles the top of the list.
    rig.panel.scroll_target = 140.0;
    rig.frame(0.016);
    let scenes = rig.rect_of(PanelHit::Header(StudioSection::Scenes));
    assert_eq!(scenes.y, TOP_H, "only the part on show can be clicked");
    assert!(scenes.h < HEADER_H, "{}", scenes.h);
    // And a click on the panel's own header, just above, does not reach it.
    let was_open = rig.panel.is_open(StudioSection::Scenes);
    rig.click(1000.0, TOP_H - 10.0);
    assert_eq!(rig.panel.is_open(StudioSection::Scenes), was_open);
}

#[test]
fn only_the_headers_in_sight_can_be_pointed_at() {
    let mut rig = Rig::new();
    tall_window(&mut rig);
    rig.wheel(-100.0);
    let hits = rig
        .panel
        .hits
        .iter()
        .filter(|(h, _)| matches!(h, PanelHit::Header(_)))
        .count();
    assert!(hits < StudioSection::ALL.len(), "{hits}");
}

#[test]
fn the_wheel_over_the_video_does_not_scroll_the_panel() {
    let mut rig = Rig::new();
    tall_window(&mut rig);
    rig.pointer.pos = Some((300.0, 200.0));
    rig.pointer.wheel = -3.0;
    rig.frame(0.016);
    assert_eq!(rig.panel.scroll, 0.0);
}

#[test]
fn the_pointer_is_wanted_over_the_close_button_and_the_headers_only() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    rig.move_to(1000.0, 700.0); // empty part of the panel
    assert!(!rig.panel.wants_pointer());
    let (x, y) = rig.rect_of(PanelHit::Close).center();
    rig.move_to(x, y);
    assert!(rig.panel.wants_pointer());
    let (x, y) = rig.rect_of(PanelHit::Header(StudioSection::Audio)).center();
    rig.move_to(x, y);
    assert!(rig.panel.wants_pointer());
}

#[test]
fn the_panel_is_laid_out_at_its_full_width_while_it_slides_in() {
    let mut rig = Rig::new();
    rig.panel.set_animations(true);
    rig.frame(0.03);
    let area = rig.panel.area().expect("opening");
    let close = rig.rect_of(PanelHit::Close);
    // The close button keeps its place relative to the panel's left edge.
    assert_eq!(close.x, area.x + PANEL_WIDTH - PAD - BUTTON);
}

#[test]
fn the_texts_come_in_the_language_of_the_model() {
    let mut rig = Rig::new();
    rig.frame(0.016);
    let english = rig.ui.vertices().len();
    rig.model.lang = dockview_domain::i18n::Lang::Es;
    rig.frame(0.016);
    // Both languages draw the title and six section names.
    assert!(rig.ui.vertices().len().abs_diff(english) < english / 2);
    let title = tr(rig.model.lang, Key::StudioTitle);
    assert_eq!(title, "Estudio");
}
