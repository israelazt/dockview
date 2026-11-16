//! The list that opens under the source card of the viewer's readouts: the
//! capture cards, the screens and the windows, to change what is shown without
//! leaving the viewer. It draws in the same coordinates as the readouts.

use dockview_domain::design::{RADIUS_CARD, RADIUS_CONTROL};
use dockview_domain::i18n::{tr, Key};
use dockview_domain::theme::Theme;
use dockview_domain::view::{Action, CurrentSource, Model};

use super::input::Pointer;
use super::text::Weight;
use super::{Rect, TextStyle, Ui};

const ICON_CHECK: char = '\u{e06c}';

const WIDTH: f32 = 320.0;
const PAD: f32 = 8.0;
const ROW_H: f32 = 32.0;
const HEADER_H: f32 = 28.0;
const GAP_BELOW: f32 = 8.0;
/// Room kept under the list for the floating bar.
const BOTTOM_ROOM: f32 = 96.0;
const MAX_H: f32 = 420.0;
const ROW_TEXT: TextStyle = TextStyle::new(13.0, Weight::Regular);
const HEADER_TEXT: TextStyle = TextStyle::new(11.0, Weight::Medium);
const WHEEL_STEP: f32 = 48.0;

/// What a row of the list stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Card(usize),
    Screen(usize),
    Window(usize),
}

impl Pick {
    fn action(self) -> Action {
        match self {
            Self::Card(i) => Action::SelectVideo(i),
            Self::Screen(i) => Action::SelectMonitor(i),
            Self::Window(i) => Action::SelectWindow(i),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Header(String),
    Item {
        pick: Pick,
        label: String,
        current: bool,
    },
    /// A group with nothing in it.
    Empty(String),
}

/// The rows of the list for what the model offers.
pub fn rows(m: &Model) -> Vec<Row> {
    let lang = m.lang;
    let now = m.sources.current;
    let card = (now == CurrentSource::Capture)
        .then_some(m.video_selected)
        .flatten();
    let screen = match now {
        CurrentSource::Monitor(i) => Some(i),
        _ => None,
    };
    let window = match now {
        CurrentSource::Window(i) => i,
        _ => None,
    };
    let mut out = Vec::new();
    let mut group =
        |title: Key, names: &[String], current: Option<usize>, pick: fn(usize) -> Pick| {
            out.push(Row::Header(tr(lang, title).to_string()));
            if names.is_empty() {
                out.push(Row::Empty(tr(lang, Key::NoDevice).to_string()));
            }
            for (i, name) in names.iter().enumerate() {
                out.push(Row::Item {
                    pick: pick(i),
                    label: name.clone(),
                    current: current == Some(i),
                });
            }
        };
    group(Key::SourceCapture, &m.video_devices, card, Pick::Card);
    group(Key::SourceScreen, &m.sources.monitors, screen, Pick::Screen);
    group(Key::SourceApp, &m.sources.windows, window, Pick::Window);
    out
}

fn row_height(row: &Row) -> f32 {
    match row {
        Row::Header(_) => HEADER_H,
        _ => ROW_H,
    }
}

/// `text` cut to `max` pixels with an ellipsis.
fn fit(ui: &mut Ui, text: &str, max: f32) -> String {
    if ui.measure(text, ROW_TEXT).0 <= max {
        return text.to_string();
    }
    let mut kept: Vec<char> = text.chars().collect();
    while !kept.is_empty() {
        kept.pop();
        let candidate = kept.iter().collect::<String>() + "…";
        if ui.measure(&candidate, ROW_TEXT).0 <= max {
            return candidate;
        }
    }
    "…".to_string()
}

#[derive(Debug, Default)]
pub struct SourcePicker {
    open: bool,
    scroll: f32,
    actions: Vec<Action>,
    hover: Option<Pick>,
    pressed: Option<Pick>,
    /// Where the list was drawn last (to tell a click outside from one inside).
    area: Option<Rect>,
}

impl SourcePicker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.scroll = 0.0;
        self.pressed = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.pressed = None;
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// Whether the pointer is over a row, or over the list at all.
    pub fn wants_pointer(&self) -> bool {
        self.open && self.hover.is_some()
    }

    /// Whether `pos` is inside the list (so what is under it must not get it).
    pub fn covers(&self, pos: Option<(f32, f32)>) -> bool {
        self.open
            && pos
                .zip(self.area)
                .is_some_and(|((x, y), r)| r.contains(x, y))
    }

    /// Draws the list under `anchor` (the source card) while it is open and
    /// handles the pointer. `anchor_hit` says the click was on the card itself,
    /// which toggles the list and so must not count as "outside".
    pub fn frame(&mut self, ui: &mut Ui, model: &Model, pointer: &Pointer, anchor: Option<Rect>) {
        let Some(anchor) = anchor.filter(|_| self.open) else {
            self.area = None;
            self.hover = None;
            if !self.open {
                self.pressed = None;
            }
            return;
        };
        let theme = &model.theme;
        let rows = rows(model);
        let content_h: f32 = rows.iter().map(row_height).sum();
        let (_, screen_h) = ui.logical_size();
        let (x, y) = (anchor.x, anchor.y + anchor.h + GAP_BELOW);
        let room = (screen_h - y - BOTTOM_ROOM).clamp(120.0, MAX_H);
        let h = (content_h + PAD * 2.0).min(room);
        let rect = Rect::new(x, y, WIDTH, h);
        self.area = Some(rect);

        let inside = pointer.pos.is_some_and(|(px, py)| rect.contains(px, py));
        let view_h = h - PAD * 2.0;
        let max_scroll = (content_h - view_h).max(0.0);
        if inside && pointer.wheel != 0.0 {
            self.scroll -= pointer.wheel * WHEEL_STEP;
        }
        self.scroll = self.scroll.clamp(0.0, max_scroll);

        ui.shadow(rect, RADIUS_CARD, 20.0, theme.shadow);
        ui.panel(rect, RADIUS_CARD, theme.surface_solid, theme.border, 1.0);
        ui.set_clip(Some(Rect::new(rect.x, rect.y + PAD, rect.w, view_h)));
        self.hover = None;
        let mut top = rect.y + PAD - self.scroll;
        for row in &rows {
            let rh = row_height(row);
            match row {
                Row::Header(title) => {
                    ui.text(title, rect.x + 16.0, top + 8.0, HEADER_TEXT, theme.text_dim);
                }
                Row::Empty(text) => {
                    let r = Rect::new(rect.x + PAD, top, rect.w - PAD * 2.0, rh);
                    ui.text(text, r.x + 8.0, top + 8.0, ROW_TEXT, theme.text_dim);
                }
                Row::Item {
                    pick,
                    label,
                    current,
                } => {
                    let r = Rect::new(rect.x + PAD, top, rect.w - PAD * 2.0, rh);
                    let visible = r.y + r.h > rect.y + PAD && r.y < rect.y + PAD + view_h;
                    let over = visible
                        && pointer
                            .pos
                            .is_some_and(|(px, py)| r.contains(px, py) && inside);
                    if over {
                        self.hover = Some(*pick);
                    }
                    draw_row(ui, theme, label, *current, over, r);
                }
            }
            top += rh;
        }
        ui.set_clip(None);
        self.handle_clicks(pointer, rect, anchor);
    }

    fn handle_clicks(&mut self, pointer: &Pointer, list: Rect, anchor: Rect) {
        if pointer.just_pressed {
            self.pressed = self.hover;
            // A press outside the list and the card closes it.
            let outside = pointer
                .pos
                .is_some_and(|(x, y)| !list.contains(x, y) && !anchor.contains(x, y));
            if outside {
                self.close();
                return;
            }
        }
        if pointer.just_released {
            if let (Some(pressed), Some(over)) = (self.pressed.take(), self.hover) {
                if pressed == over {
                    self.actions.push(over.action());
                    self.close();
                }
            }
        }
    }
}

fn draw_row(ui: &mut Ui, theme: &Theme, label: &str, current: bool, over: bool, r: Rect) {
    if current {
        ui.rect(r, RADIUS_CONTROL, theme.brand.with_alpha(0.12));
    } else if over {
        ui.rect(r, RADIUS_CONTROL, theme.hover);
    }
    let text = fit(ui, label, r.w - 16.0 - 28.0);
    let (_, h) = ui.measure(&text, ROW_TEXT);
    let color = if current { theme.brand } else { theme.text };
    ui.text(&text, r.x + 10.0, r.y + (r.h - h) / 2.0, ROW_TEXT, color);
    if current {
        ui.icon(
            ICON_CHECK,
            r.x + r.w - 16.0,
            r.y + r.h / 2.0,
            15.0,
            theme.brand,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_model::model;
    use dockview_domain::i18n::Lang;

    fn offering() -> Model {
        let mut m = model();
        m.lang = Lang::En;
        m.video_devices = vec!["UGREEN 25173".into(), "Integrated Camera".into()];
        m.video_selected = Some(0);
        m.sources.monitors = vec!["Monitor 1".into()];
        m.sources.windows = vec!["Game".into(), "Browser".into()];
        m
    }

    const ANCHOR: Rect = Rect::new(16.0, 12.0, 200.0, 52.0);

    fn draw(p: &mut SourcePicker, m: &Model, pointer: &Pointer) -> Ui {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        p.frame(&mut ui, m, pointer, Some(ANCHOR));
        ui
    }

    /// The centre of the row of `pick`, as the list lays it out.
    fn centre(m: &Model, pick: Pick) -> (f32, f32) {
        let x = ANCHOR.x + PAD + (WIDTH - PAD * 2.0) / 2.0;
        let mut y = ANCHOR.y + ANCHOR.h + GAP_BELOW + PAD;
        for row in rows(m) {
            let h = row_height(&row);
            if matches!(&row, Row::Item { pick: p, .. } if *p == pick) {
                return (x, y + h / 2.0);
            }
            y += h;
        }
        panic!("no row for {pick:?}");
    }

    fn click(p: &mut SourcePicker, m: &Model, at: (f32, f32)) {
        let mut pt = Pointer {
            pos: Some(at),
            ..Pointer::default()
        };
        draw(p, m, &pt);
        pt.press();
        draw(p, m, &pt);
        pt.end_frame();
        pt.release();
        draw(p, m, &pt);
    }

    #[test]
    fn the_rows_group_cards_screens_and_windows_and_mark_the_current_one() {
        let rows = rows(&offering());
        let headers: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Header(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(headers, ["Capture card", "Screen", "Application window"]);
        let current: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Item {
                    pick,
                    current: true,
                    ..
                } => Some(*pick),
                _ => None,
            })
            .collect();
        assert_eq!(current, [Pick::Card(0)]);
        let mut m = offering();
        m.sources.current = CurrentSource::Window(Some(1));
        let on = rows_current(&m);
        assert_eq!(on, [Pick::Window(1)]);
        m.sources.current = CurrentSource::Monitor(0);
        assert_eq!(rows_current(&m), [Pick::Screen(0)]);
    }

    fn rows_current(m: &Model) -> Vec<Pick> {
        rows(m)
            .into_iter()
            .filter_map(|r| match r {
                Row::Item {
                    pick,
                    current: true,
                    ..
                } => Some(pick),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_empty_group_says_so() {
        let mut m = offering();
        m.sources.windows.clear();
        assert!(rows(&m)
            .iter()
            .any(|r| matches!(r, Row::Empty(t) if t == "No device")));
    }

    #[test]
    fn it_draws_only_while_open() {
        let m = offering();
        let mut p = SourcePicker::new();
        assert!(draw(&mut p, &m, &Pointer::default()).vertices().is_empty());
        p.toggle();
        assert!(!draw(&mut p, &m, &Pointer::default()).vertices().is_empty());
        p.toggle();
        assert!(draw(&mut p, &m, &Pointer::default()).vertices().is_empty());
    }

    #[test]
    fn choosing_a_row_asks_for_that_source_and_closes() {
        let m = offering();
        for (pick, action) in [
            (Pick::Card(1), Action::SelectVideo(1)),
            (Pick::Screen(0), Action::SelectMonitor(0)),
            (Pick::Window(1), Action::SelectWindow(1)),
        ] {
            let mut p = SourcePicker::new();
            p.toggle();
            click(&mut p, &m, centre(&m, pick));
            assert_eq!(p.take_actions(), [action], "{pick:?}");
            assert!(!p.is_open());
        }
    }

    #[test]
    fn a_click_outside_closes_it_and_a_click_on_the_card_is_left_to_the_card() {
        let m = offering();
        let mut p = SourcePicker::new();
        p.toggle();
        click(&mut p, &m, (900.0, 500.0));
        assert!(!p.is_open());
        assert!(p.take_actions().is_empty());
        // On the card the picker stays as it is: the card toggles it itself.
        let mut p = SourcePicker::new();
        p.toggle();
        click(&mut p, &m, (ANCHOR.x + 10.0, ANCHOR.y + 10.0));
        assert!(p.is_open());
    }

    #[test]
    fn letting_go_on_another_row_chooses_nothing() {
        let m = offering();
        let mut p = SourcePicker::new();
        p.toggle();
        let a = centre(&m, Pick::Card(1));
        let b = centre(&m, Pick::Window(0));
        let mut pt = Pointer {
            pos: Some(a),
            ..Pointer::default()
        };
        draw(&mut p, &m, &pt);
        pt.press();
        draw(&mut p, &m, &pt);
        pt.end_frame();
        pt.pos = Some(b);
        draw(&mut p, &m, &pt);
        pt.release();
        draw(&mut p, &m, &pt);
        assert!(p.take_actions().is_empty());
    }

    #[test]
    fn a_long_list_scrolls_with_the_wheel_and_stops_at_its_ends() {
        let mut m = offering();
        m.sources.windows = (0..40).map(|i| format!("Window {i}")).collect();
        let mut p = SourcePicker::new();
        p.toggle();
        let inside = (ANCHOR.x + 40.0, ANCHOR.y + ANCHOR.h + 60.0);
        let mut pt = Pointer {
            pos: Some(inside),
            wheel: -3.0, // down
            ..Pointer::default()
        };
        draw(&mut p, &m, &pt);
        assert!(p.scroll > 0.0);
        let after = p.scroll;
        pt.wheel = 3.0; // up, further than it came
        draw(&mut p, &m, &pt);
        pt.wheel = 3.0;
        draw(&mut p, &m, &pt);
        assert!(p.scroll < after && p.scroll >= 0.0);
        pt.wheel = -500.0;
        draw(&mut p, &m, &pt);
        let content: f32 = rows(&m).iter().map(row_height).sum();
        assert!(p.scroll <= content, "{}", p.scroll);
    }

    #[test]
    fn it_knows_what_it_covers() {
        let m = offering();
        let mut p = SourcePicker::new();
        assert!(!p.covers(Some((100.0, 100.0))));
        p.toggle();
        draw(&mut p, &m, &Pointer::default());
        assert!(p.covers(Some((ANCHOR.x + 20.0, ANCHOR.y + ANCHOR.h + 40.0))));
        assert!(!p.covers(Some((900.0, 600.0))));
        assert!(!p.covers(None));
    }
}
