//! The bar under the picture in the Studio: how the scenes change into one
//! another (a cut or a smooth glide) and for how long. What the bar offers is
//! a plain function of the model, so it is tested without a window; the drawing
//! only lays the choices in a row and turns a press into an [`Action`].

use dockview_domain::design::RADIUS_CARD;
use dockview_domain::i18n::{tr, Key, Lang};
use dockview_domain::settings::{SceneTransition, TRANSITION_CHOICES_MS};
use dockview_domain::view::{Action, Model};

use super::input::Pointer;
use super::text::Weight;
use super::{Rect, TextStyle, Ui};

/// Height of the bar, in logical pixels.
pub const BAR_H: f32 = 52.0;
const PAD: f32 = 12.0;
const CHIP_H: f32 = 30.0;
const CHIP_PAD: f32 = 12.0;
const CHIP_GAP: f32 = 4.0;
const GROUP_GAP: f32 = 14.0;
const TITLE: TextStyle = TextStyle::new(12.0, Weight::SemiBold);
const CHIP: TextStyle = TextStyle::new(12.5, Weight::Medium);

/// One choice of the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub label: String,
    /// What pressing it asks for.
    pub action: Action,
    pub selected: bool,
    /// A choice that does not apply right now is shown dim and does nothing.
    pub enabled: bool,
}

/// The choices of the bar, in groups drawn left to right.
#[derive(Debug, Clone, PartialEq)]
pub struct BarData {
    pub title: String,
    pub groups: Vec<Vec<Choice>>,
}

/// A length in seconds as the language writes it: `0.4 s` or `0,4 s`.
pub fn seconds_label(lang: Lang, ms: u32) -> String {
    let text = format!("{:.1} s", ms as f32 / 1000.0);
    match lang {
        Lang::Es => text.replace('.', ","),
        _ => text,
    }
}

pub fn data(m: &Model) -> BarData {
    let lang = m.lang;
    let smooth = m.scenes.transition == SceneTransition::Smooth;
    let kinds = [
        (SceneTransition::Smooth, Key::TransitionSmooth),
        (SceneTransition::Cut, Key::TransitionCut),
    ];
    let kind_group = kinds
        .into_iter()
        .map(|(kind, key)| Choice {
            label: tr(lang, key).to_string(),
            action: Action::SetSceneTransition(kind),
            selected: m.scenes.transition == kind,
            enabled: true,
        })
        .collect();
    // The length only matters for a smooth change.
    let length_group = TRANSITION_CHOICES_MS
        .into_iter()
        .map(|ms| Choice {
            label: seconds_label(lang, ms),
            action: Action::SetTransitionMs(ms),
            selected: m.scenes.transition_ms == ms,
            enabled: smooth,
        })
        .collect();
    BarData {
        title: tr(lang, Key::TransitionBarTitle).to_string(),
        groups: vec![kind_group, length_group],
    }
}

/// Where each choice goes: rectangles in the same order as the groups, from
/// `left`, centred in a bar whose top is `y`; `widths` has each choice's width.
pub fn layout(left: f32, y: f32, groups: &[Vec<f32>]) -> Vec<Vec<Rect>> {
    let top = y + (BAR_H - CHIP_H) / 2.0;
    let mut x = left;
    groups
        .iter()
        .map(|widths| {
            let row = widths
                .iter()
                .map(|w| {
                    let r = Rect::new(x, top, *w, CHIP_H);
                    x += w + CHIP_GAP;
                    r
                })
                .collect();
            x += GROUP_GAP - CHIP_GAP;
            row
        })
        .collect()
}

#[derive(Default)]
pub struct CanvasBar {
    actions: Vec<Action>,
    hover: bool,
}

impl CanvasBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// Whether the pointer is over a choice that does something (hand cursor).
    pub fn wants_pointer(&self) -> bool {
        self.hover
    }

    /// The bar is not on screen: no choice is under the pointer.
    pub fn frame_hidden(&mut self) {
        self.hover = false;
    }

    /// Draws the bar as a card in `area` and handles the pointer, which is in
    /// the same coordinates as `area`.
    pub fn frame(&mut self, ui: &mut Ui, m: &Model, area: Rect, pointer: &Pointer) {
        let t = &m.theme;
        let d = data(m);
        ui.panel(area, RADIUS_CARD, t.surface_solid, t.border, 1.0);
        let title_h = ui.measure("Ag", TITLE).1;
        let title_w = ui.text(
            &d.title,
            area.x + PAD,
            area.y + (area.h - title_h) / 2.0,
            TITLE,
            t.text_dim,
        );
        let widths: Vec<Vec<f32>> = d
            .groups
            .iter()
            .map(|g| {
                g.iter()
                    .map(|c| ui.measure(&c.label, CHIP).0 + CHIP_PAD * 2.0)
                    .collect()
            })
            .collect();
        let rects = layout(area.x + PAD + title_w + GROUP_GAP, area.y, &widths);
        let line_h = ui.measure("Ag", CHIP).1;
        self.hover = false;
        for (group, rects) in d.groups.iter().zip(&rects) {
            for (choice, r) in group.iter().zip(rects) {
                let over = pointer.pos.is_some_and(|(x, y)| r.contains(x, y));
                if choice.selected {
                    ui.rect(*r, 9.0, t.segment_active);
                } else if over && choice.enabled {
                    ui.rect(*r, 9.0, t.hover);
                } else {
                    ui.rect(*r, 9.0, t.segment_track);
                }
                let color = match (choice.selected, choice.enabled) {
                    (true, true) => t.text,
                    (false, true) => t.text_dim,
                    (_, false) => t.text_dim.with_alpha(0.45),
                };
                let w = ui.measure(&choice.label, CHIP).0;
                ui.text(
                    &choice.label,
                    r.x + (r.w - w) / 2.0,
                    r.y + (r.h - line_h) / 2.0,
                    CHIP,
                    color,
                );
                if over && choice.enabled {
                    self.hover = true;
                    if pointer.just_pressed && !choice.selected {
                        self.actions.push(choice.action.clone());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_model::model;

    #[test]
    fn it_offers_the_two_kinds_and_three_lengths() {
        let d = data(&model());
        assert_eq!(d.groups.len(), 2);
        let kinds: Vec<_> = d.groups[0].iter().map(|c| c.label.as_str()).collect();
        assert_eq!(kinds, ["Smooth", "Cut"]);
        let lengths: Vec<_> = d.groups[1].iter().map(|c| c.label.as_str()).collect();
        assert_eq!(lengths, ["0.2 s", "0.4 s", "0.8 s"]);
    }

    #[test]
    fn the_chosen_kind_and_length_are_marked() {
        let mut m = model();
        m.scenes.transition = SceneTransition::Smooth;
        m.scenes.transition_ms = 800;
        let d = data(&m);
        assert_eq!(
            d.groups[0].iter().map(|c| c.selected).collect::<Vec<_>>(),
            [true, false]
        );
        assert_eq!(
            d.groups[1].iter().map(|c| c.selected).collect::<Vec<_>>(),
            [false, false, true]
        );
        m.scenes.transition = SceneTransition::Cut;
        assert!(data(&m).groups[0][1].selected);
    }

    #[test]
    fn a_cut_has_no_length_so_the_lengths_are_off() {
        let mut m = model();
        m.scenes.transition = SceneTransition::Cut;
        assert!(data(&m).groups[1].iter().all(|c| !c.enabled));
        assert!(data(&m).groups[0].iter().all(|c| c.enabled));
        m.scenes.transition = SceneTransition::Smooth;
        assert!(data(&m).groups[1].iter().all(|c| c.enabled));
    }

    #[test]
    fn lengths_are_written_the_way_the_language_does() {
        assert_eq!(seconds_label(Lang::En, 400), "0.4 s");
        assert_eq!(seconds_label(Lang::Es, 400), "0,4 s");
        assert_eq!(seconds_label(Lang::Es, 1500), "1,5 s");
    }

    #[test]
    fn the_choices_go_in_a_row_without_overlapping_and_centred_in_the_bar() {
        let rects = layout(100.0, 10.0, &[vec![60.0, 40.0], vec![50.0]]);
        assert_eq!(rects[0][0].x, 100.0);
        assert_eq!(rects[0][1].x, 100.0 + 60.0 + CHIP_GAP);
        assert_eq!(rects[1][0].x, rects[0][1].x + 40.0 + GROUP_GAP);
        assert_eq!(rects[0][0].y, 10.0 + (BAR_H - CHIP_H) / 2.0);
    }

    fn press_at(bar: &mut CanvasBar, m: &Model, area: Rect, x: f32, y: f32) -> Vec<Action> {
        let mut ui = Ui::new();
        ui.begin((1200, 800), 1.0);
        let pointer = Pointer {
            pos: Some((x, y)),
            down: true,
            just_pressed: true,
            ..Pointer::default()
        };
        bar.frame(&mut ui, m, area, &pointer);
        bar.take_actions()
    }

    /// The centre of choice `i` of group `g`, as the drawing places it.
    fn centre_of(m: &Model, area: Rect, g: usize, i: usize) -> (f32, f32) {
        let mut ui = Ui::new();
        ui.begin((1200, 800), 1.0);
        let d = data(m);
        let title_w = ui.measure(&d.title, TITLE).0;
        let widths: Vec<Vec<f32>> = d
            .groups
            .iter()
            .map(|g| {
                g.iter()
                    .map(|c| ui.measure(&c.label, CHIP).0 + CHIP_PAD * 2.0)
                    .collect()
            })
            .collect();
        let r = layout(area.x + PAD + title_w + GROUP_GAP, area.y, &widths)[g][i];
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    #[test]
    fn pressing_a_choice_asks_for_it_and_pressing_the_chosen_one_does_nothing() {
        let mut m = model();
        m.scenes.transition = SceneTransition::Smooth;
        m.scenes.transition_ms = 400;
        let area = Rect::new(20.0, 700.0, 700.0, BAR_H);
        let (x, y) = centre_of(&m, area, 0, 1);
        assert_eq!(
            press_at(&mut CanvasBar::new(), &m, area, x, y),
            [Action::SetSceneTransition(SceneTransition::Cut)]
        );
        let (x, y) = centre_of(&m, area, 1, 2);
        assert_eq!(
            press_at(&mut CanvasBar::new(), &m, area, x, y),
            [Action::SetTransitionMs(800)]
        );
        let (x, y) = centre_of(&m, area, 1, 1);
        assert!(press_at(&mut CanvasBar::new(), &m, area, x, y).is_empty());
    }

    #[test]
    fn with_a_cut_the_lengths_do_not_answer_to_a_press() {
        let mut m = model();
        m.scenes.transition = SceneTransition::Cut;
        let area = Rect::new(20.0, 700.0, 700.0, BAR_H);
        let (x, y) = centre_of(&m, area, 1, 0);
        let mut bar = CanvasBar::new();
        assert!(press_at(&mut bar, &m, area, x, y).is_empty());
        assert!(!bar.wants_pointer());
    }
}
