//! The icon rail on the left: shortcuts to the sections of the Studio panel and
//! to the log. It draws in its own coordinates (the shell leaves room above it)
//! and turns clicks into actions.

use dockview_domain::i18n::{tr, Key};
use dockview_domain::navigation::NavTab;
use dockview_domain::theme::Theme;
use dockview_domain::view::{Action, Model};
use dockview_domain::window_chrome::RAIL_WIDTH;

use super::input::Pointer;
use super::studio::StudioSection;
use super::text::Weight;
use super::{Rect, TextStyle, Ui};

const ICON_SOURCES: char = '\u{e1a5}';
const ICON_AUDIO: char = '\u{e162}';
const ICON_STATS: char = '\u{e038}';
const ICON_SCENES: char = '\u{e52a}';
const ICON_LOG: char = '\u{e181}';

const BUTTON: f32 = 34.0;
const ICON_SIZE: f32 = 17.0;
const TOP_PAD: f32 = 12.0;
const BOTTOM_PAD: f32 = 12.0;
const GAP: f32 = 6.0;
/// How long the pointer rests on an icon before its label shows, in seconds.
const TIP_DELAY: f32 = 0.45;
const TIP_STYLE: TextStyle = TextStyle::new(12.5, Weight::Medium);

/// What an icon of the rail stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailItem {
    /// A section of the Studio panel.
    Section(StudioSection),
    /// The log file, shown in the file manager.
    Log,
}

/// Top to bottom: the sections, then (at the foot) the log.
const TOP: [RailItem; 4] = [
    RailItem::Section(StudioSection::Sources),
    RailItem::Section(StudioSection::Audio),
    RailItem::Section(StudioSection::Stats),
    RailItem::Section(StudioSection::Scenes),
];
const BOTTOM: [RailItem; 1] = [RailItem::Log];

impl RailItem {
    fn glyph(self) -> char {
        match self {
            Self::Section(StudioSection::Sources) => ICON_SOURCES,
            Self::Section(StudioSection::Audio) => ICON_AUDIO,
            Self::Section(StudioSection::Stats) => ICON_STATS,
            Self::Section(_) => ICON_SCENES,
            Self::Log => ICON_LOG,
        }
    }

    fn label(self) -> Key {
        match self {
            Self::Section(section) => section.title(),
            Self::Log => Key::RailLog,
        }
    }
}

/// Where each icon is, for a rail `height` tall.
pub fn layout(height: f32) -> Vec<(RailItem, Rect)> {
    let x = (RAIL_WIDTH - BUTTON) / 2.0;
    let mut out = Vec::new();
    let mut y = TOP_PAD;
    for item in TOP {
        out.push((item, Rect::new(x, y, BUTTON, BUTTON)));
        y += BUTTON + GAP;
    }
    let mut y = height - BOTTOM_PAD - BUTTON;
    for item in BOTTOM.iter().rev() {
        out.push((*item, Rect::new(x, y, BUTTON, BUTTON)));
        y -= BUTTON + GAP;
    }
    out
}

#[derive(Debug, Default)]
pub struct Rail {
    actions: Vec<Action>,
    section: Option<StudioSection>,
    log: bool,
    hover: Option<RailItem>,
    pressed: Option<RailItem>,
    /// How long the pointer has rested on `hover`.
    rest: f32,
}

impl Rail {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// The section of the Studio panel an icon asked to be opened, once.
    pub fn take_studio_request(&mut self) -> Option<StudioSection> {
        self.section.take()
    }

    /// Whether the log was asked for, once.
    pub fn take_log_request(&mut self) -> bool {
        std::mem::take(&mut self.log)
    }

    /// Whether the pointer is over an icon.
    pub fn wants_pointer(&self) -> bool {
        self.hover.is_some()
    }

    /// Draws the rail down the left of the area the shell gave it and handles
    /// the pointer, which is in the same coordinates. `active` is the section
    /// the panel is showing, if it is shown.
    pub fn frame(
        &mut self,
        ui: &mut Ui,
        model: &Model,
        pointer: &Pointer,
        active: Option<StudioSection>,
        dt: f32,
    ) {
        let theme = &model.theme;
        let (_, height) = ui.logical_size();
        ui.rect(Rect::new(0.0, 0.0, RAIL_WIDTH, height), 0.0, theme.chrome);
        ui.rect(
            Rect::new(RAIL_WIDTH - 1.0, 0.0, 1.0, height),
            0.0,
            theme.chrome_line,
        );

        let items = layout(height);
        let over = pointer.pos.and_then(|(x, y)| {
            items
                .iter()
                .find(|(_, r)| r.contains(x, y))
                .map(|(item, r)| (*item, *r))
        });
        let hover = over.map(|(item, _)| item);
        self.rest = if hover.is_some() && hover == self.hover && !pointer.down {
            self.rest + dt
        } else {
            0.0
        };
        self.hover = hover;
        self.handle_clicks(pointer);

        for (item, rect) in &items {
            let on = matches!((item, active), (RailItem::Section(a), Some(b)) if *a == b);
            self.draw_item(ui, theme, *item, *rect, on);
        }
        if let (Some((item, rect)), true) = (over, self.rest >= TIP_DELAY) {
            draw_tip(ui, theme, tr(model.lang, item.label()), rect);
        }
    }

    fn handle_clicks(&mut self, pointer: &Pointer) {
        if pointer.just_pressed {
            self.pressed = self.hover;
        }
        if pointer.just_released {
            if let (Some(pressed), Some(over)) = (self.pressed.take(), self.hover) {
                if pressed == over {
                    match over {
                        RailItem::Section(section) => {
                            self.actions.push(Action::GoTo(NavTab::Studio));
                            self.section = Some(section);
                        }
                        RailItem::Log => self.log = true,
                    }
                }
            }
            self.pressed = None;
        }
    }

    fn draw_item(&self, ui: &mut Ui, theme: &Theme, item: RailItem, r: Rect, on: bool) {
        let hovered = self.hover == Some(item);
        if on {
            ui.panel(
                r,
                10.0,
                theme.brand.with_alpha(0.12),
                theme.brand.with_alpha(0.35),
                1.0,
            );
            // The mark on the left edge of the rail.
            ui.rect(Rect::new(0.0, r.y + 7.0, 3.0, r.h - 14.0), 1.5, theme.brand);
        } else if hovered {
            ui.rect(r, 10.0, theme.hover);
        }
        let color = if on {
            theme.brand
        } else if hovered {
            theme.text
        } else {
            theme.text_dim
        };
        let (cx, cy) = r.center();
        ui.icon(item.glyph(), cx, cy, ICON_SIZE, color);
    }
}

/// The icon's name, to the right of it.
fn draw_tip(ui: &mut Ui, theme: &Theme, text: &str, anchor: Rect) {
    let (w, h) = ui.measure(text, TIP_STYLE);
    let r = Rect::new(
        RAIL_WIDTH + 6.0,
        anchor.y + (anchor.h - (h + 12.0)) / 2.0,
        w + 20.0,
        h + 12.0,
    );
    ui.shadow(r, 8.0, 12.0, theme.shadow);
    ui.panel(r, 8.0, theme.surface_solid, theme.border, 1.0);
    ui.text(text, r.x + 10.0, r.y + 6.0, TIP_STYLE, theme.text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_model::model;

    const HEIGHT: f32 = 600.0;

    fn draw(rail: &mut Rail, m: &Model, p: &Pointer, active: Option<StudioSection>, dt: f32) -> Ui {
        let mut ui = Ui::new();
        ui.begin((1280, 600), 1.0);
        rail.frame(&mut ui, m, p, active, dt);
        ui
    }

    fn centre(item: RailItem) -> (f32, f32) {
        layout(HEIGHT)
            .iter()
            .find(|(i, _)| *i == item)
            .unwrap()
            .1
            .center()
    }

    fn click(rail: &mut Rail, m: &Model, at: (f32, f32)) {
        let mut p = Pointer {
            pos: Some(at),
            ..Pointer::default()
        };
        draw(rail, m, &p, None, 0.016);
        p.press();
        draw(rail, m, &p, None, 0.016);
        p.end_frame();
        p.release();
        draw(rail, m, &p, None, 0.016);
    }

    #[test]
    fn the_icons_stack_from_the_top_and_the_log_sits_at_the_foot() {
        let l = layout(HEIGHT);
        assert_eq!(l.len(), 5);
        assert_eq!(l[0].0, RailItem::Section(StudioSection::Sources));
        assert_eq!(l[3].0, RailItem::Section(StudioSection::Scenes));
        assert!(l[1].1.y > l[0].1.y && l[3].1.y > l[2].1.y);
        let log = l.iter().find(|(i, _)| *i == RailItem::Log).unwrap().1;
        assert_eq!(log.y + log.h, HEIGHT - BOTTOM_PAD);
        assert!(l.iter().all(|(_, r)| r.x + r.w <= RAIL_WIDTH));
    }

    #[test]
    fn a_short_rail_keeps_the_log_below_the_sections_or_overlaps_nothing_it_can_avoid() {
        // Nothing panics, and every icon stays inside the rail's width.
        for h in [0.0, 50.0, 120.0, 400.0] {
            assert_eq!(layout(h).len(), 5);
        }
    }

    #[test]
    fn clicking_a_section_icon_opens_the_panel_on_that_section() {
        let m = model();
        let mut rail = Rail::new();
        click(
            &mut rail,
            &m,
            centre(RailItem::Section(StudioSection::Audio)),
        );
        assert_eq!(rail.take_actions(), [Action::GoTo(NavTab::Studio)]);
        assert_eq!(rail.take_studio_request(), Some(StudioSection::Audio));
        assert_eq!(rail.take_studio_request(), None, "asked once");
        assert!(!rail.take_log_request());
    }

    #[test]
    fn clicking_the_terminal_asks_for_the_log_and_nothing_else() {
        let m = model();
        let mut rail = Rail::new();
        click(&mut rail, &m, centre(RailItem::Log));
        assert!(rail.take_log_request());
        assert!(!rail.take_log_request(), "asked once");
        assert!(rail.take_actions().is_empty());
        assert_eq!(rail.take_studio_request(), None);
    }

    #[test]
    fn a_click_that_ends_elsewhere_does_nothing() {
        let m = model();
        let mut rail = Rail::new();
        let a = centre(RailItem::Section(StudioSection::Stats));
        let mut p = Pointer {
            pos: Some(a),
            ..Pointer::default()
        };
        draw(&mut rail, &m, &p, None, 0.016);
        p.press();
        draw(&mut rail, &m, &p, None, 0.016);
        p.end_frame();
        p.pos = Some((300.0, 300.0));
        draw(&mut rail, &m, &p, None, 0.016);
        p.release();
        draw(&mut rail, &m, &p, None, 0.016);
        assert!(rail.take_actions().is_empty());
        assert_eq!(rail.take_studio_request(), None);
    }

    #[test]
    fn the_active_section_is_marked_and_the_others_are_not() {
        let m = model();
        let plain = draw(&mut Rail::new(), &m, &Pointer::default(), None, 0.0);
        let on = draw(
            &mut Rail::new(),
            &m,
            &Pointer::default(),
            Some(StudioSection::Audio),
            0.0,
        );
        assert_ne!(plain.vertices(), on.vertices());
        let other = draw(
            &mut Rail::new(),
            &m,
            &Pointer::default(),
            Some(StudioSection::Scenes),
            0.0,
        );
        assert_ne!(on.vertices(), other.vertices());
        // A section with no icon (recording, settings) marks nothing.
        let none = draw(
            &mut Rail::new(),
            &m,
            &Pointer::default(),
            Some(StudioSection::Recording),
            0.0,
        );
        assert_eq!(plain.vertices(), none.vertices());
    }

    #[test]
    fn the_label_shows_only_after_the_pointer_rests() {
        let m = model();
        let mut rail = Rail::new();
        let p = Pointer {
            pos: Some(centre(RailItem::Log)),
            ..Pointer::default()
        };
        let first = draw(&mut rail, &m, &p, None, 0.1).vertices().len();
        let still = draw(&mut rail, &m, &p, None, 0.1).vertices().len();
        assert_eq!(first, still, "not yet");
        let shown = draw(&mut rail, &m, &p, None, 0.5).vertices().len();
        assert!(shown > still, "the label appears");
        assert!(rail.wants_pointer());
        // Leaving hides it and the hand cursor goes.
        let gone = draw(&mut rail, &m, &Pointer::default(), None, 0.016);
        assert!(!rail.wants_pointer());
        assert!(gone.vertices().len() <= still);
    }
}
