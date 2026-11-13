//! The Studio panel: a column docked on the right of the window with the tools
//! that do not fit in the floating pill. The video shrinks to the room that is
//! left (the window computes that with `dockview_domain::layout::studio_layout`
//! and tells the renderer); the panel is drawn into the room it took.
//!
//! It is a list of sections that fold and unfold, under a header with a close
//! button, and it scrolls with the wheel when the sections do not fit. Like the
//! controls it is immediate mode: each frame reads the pointer, decides what is
//! hovered with the rectangles of the previous frame, emits [`Action`]s and
//! draws. What goes inside each section is made block by block (the bodies are
//! empty for now).

mod route;
mod scroll;
mod sections;

use std::collections::HashMap;

use dockview_domain::i18n::{tr, Key};
use dockview_domain::theme::Theme;
use dockview_domain::view::{Action, Model};

pub use route::route_pointer;
pub use sections::StudioSection;

use crate::input::Pointer;
use crate::motion::{ease_out_cubic, lerp_color, move_toward, smooth};
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};
use sections::{stack, SectionRow, HEADER_H};

/// Height of the panel's own header (the title and the close button).
const TOP_H: f32 = 52.0;
const PAD: f32 = 16.0;
const BUTTON: f32 = 32.0;
const ICON_SIZE: f32 = 17.0;
/// Time the panel takes to open or close, in seconds.
const OPEN_SECS: f32 = 0.18;
/// Smoothing rates (higher is faster).
const EXPAND_RATE: f32 = 18.0;
const SCROLL_RATE: f32 = 26.0;
const HOVER_RATE: f32 = 22.0;

const TITLE: TextStyle = TextStyle::new(14.0, Weight::SemiBold);
const SECTION: TextStyle = TextStyle::new(13.0, Weight::Medium);
const NOTE: TextStyle = TextStyle::new(12.5, Weight::Regular);

// Lucide code points.
const ICON_CHEVRON_DOWN: char = '\u{e06d}';
const ICON_CHEVRON_RIGHT: char = '\u{e06f}';
const ICON_CLOSE: char = '\u{e1b2}';

/// Something in the panel that can be pointed at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PanelHit {
    Close,
    Header(StudioSection),
}

pub struct StudioPanel {
    /// Which sections are meant to be open.
    open: [bool; 6],
    /// How open each one is now (0 to 1), moving towards `open`.
    amount: [f32; 6],
    scroll: f32,
    scroll_target: f32,
    /// How far the panel is opened, 0 (closed) to 1, linear in time.
    progress: f32,
    animations: bool,
    /// Rectangles of the previous frame; later entries are on top.
    hits: Vec<(PanelHit, Rect)>,
    next_hits: Vec<(PanelHit, Rect)>,
    hovered: Option<PanelHit>,
    pressed: Option<PanelHit>,
    /// Smoothed hover amount of each pointed-at thing.
    hover: HashMap<PanelHit, f32>,
    actions: Vec<Action>,
    /// The part of the panel on screen in the last frame.
    area: Option<Rect>,
    dt: f32,
}

impl StudioPanel {
    pub fn new() -> Self {
        Self {
            open: [false; 6],
            amount: [0.0; 6],
            scroll: 0.0,
            scroll_target: 0.0,
            progress: 0.0,
            animations: true,
            hits: Vec::new(),
            next_hits: Vec::new(),
            hovered: None,
            pressed: None,
            hover: HashMap::new(),
            actions: Vec::new(),
            area: None,
            dt: 0.0,
        }
    }

    /// Turns motion on or off (off snaps every change, honouring the Windows
    /// "show animations" setting).
    pub fn set_animations(&mut self, on: bool) {
        self.animations = on;
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    // -- Opening and closing -----------------------------------------------

    /// Moves the opening along: towards open when `visible`, closed otherwise.
    /// With `instant` (full screen, where no panel is wanted) it jumps.
    pub fn update(&mut self, visible: bool, instant: bool, dt: f32) {
        let target = if visible { 1.0 } else { 0.0 };
        self.progress = if self.animations && !instant {
            move_toward(self.progress, target, dt / OPEN_SECS)
        } else {
            target
        };
    }

    /// How open the panel is for the layout, 0 to 1, with a fast start and a
    /// gentle landing; the width and the fade both follow it.
    pub fn openness(&self) -> f32 {
        ease_out_cubic(self.progress)
    }

    /// Whether anything is moving (so the window keeps drawing frames).
    pub fn is_animating(&self) -> bool {
        let opening = self.progress > 0.0 && self.progress < 1.0;
        let folding = self
            .open
            .iter()
            .zip(&self.amount)
            .any(|(open, amount)| *amount != if *open { 1.0 } else { 0.0 });
        let scrolling = (self.scroll - self.scroll_target).abs() > 0.01;
        let fading = self.hover.values().any(|h| *h > 0.0 && *h < 1.0);
        opening || folding || scrolling || fading
    }

    // -- The pointer -------------------------------------------------------

    /// Whether the pointer is over something that can be clicked.
    pub fn wants_pointer(&self) -> bool {
        self.hovered.is_some()
    }

    /// The part of the panel that was on screen in the last frame.
    pub fn area(&self) -> Option<Rect> {
        self.area
    }

    // -- A frame -----------------------------------------------------------

    /// Runs one frame: input, then drawing. `area` is the room the panel has
    /// now (from the layout, in logical pixels; `None` when it is closed);
    /// `full_width` is what it is when fully open. The panel is laid out at its
    /// full width and slides in by its left edge.
    pub fn frame(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        area: Option<Rect>,
        full_width: f32,
        pointer: &Pointer,
        dt: f32,
    ) {
        self.dt = dt;
        self.area = area;
        let Some(visible) = area else {
            self.hits.clear();
            self.hovered = None;
            self.pressed = None;
            return;
        };
        let panel = Rect::new(visible.x, visible.y, full_width, visible.h);
        let list = Rect::new(
            panel.x,
            panel.y + TOP_H,
            panel.w,
            (panel.h - TOP_H).max(0.0),
        );

        self.step_sections();
        let (rows, content_h) = stack(&self.amount);
        self.scroll_target = scroll::clamp_scroll(self.scroll_target, content_h, list.h);
        self.hovered = self.hit_at(pointer.pos);
        self.interact(pointer, visible, content_h, list.h);
        self.scroll = if self.animations {
            smooth(self.scroll, self.scroll_target, dt, SCROLL_RATE)
        } else {
            self.scroll_target
        };

        let t = &m.theme;
        let fade = self.openness();
        self.next_hits.clear();
        ui.set_opacity(fade);
        self.draw_frame(ui, t, panel);
        self.draw_top(ui, m, panel);
        self.draw_sections(ui, m, &rows, list);
        self.draw_scroll_bar(ui, t, list, content_h);
        ui.set_clip(None);
        ui.set_opacity(1.0);
        std::mem::swap(&mut self.hits, &mut self.next_hits);
    }

    /// Moves each section's opening towards what it is meant to be.
    fn step_sections(&mut self) {
        for (amount, open) in self.amount.iter_mut().zip(self.open) {
            let target = if open { 1.0 } else { 0.0 };
            *amount = if self.animations {
                smooth(*amount, target, self.dt, EXPAND_RATE)
            } else {
                target
            };
        }
    }

    fn hit_at(&self, pos: Option<(f32, f32)>) -> Option<PanelHit> {
        let (x, y) = pos?;
        self.hits
            .iter()
            .rev()
            .find(|(_, rect)| rect.contains(x, y))
            .map(|(hit, _)| *hit)
    }

    fn interact(&mut self, pointer: &Pointer, visible: Rect, content_h: f32, list_h: f32) {
        if pointer.just_pressed {
            self.pressed = self.hovered;
        }
        if pointer.just_released {
            if let (Some(pressed), Some(hovered)) = (self.pressed, self.hovered) {
                if pressed == hovered {
                    self.activate(pressed);
                }
            }
            self.pressed = None;
        }
        let over_panel = pointer.pos.is_some_and(|(x, y)| visible.contains(x, y));
        if pointer.wheel != 0.0 && over_panel {
            self.scroll_target =
                scroll::scroll_by(self.scroll_target, pointer.wheel, content_h, list_h);
        }
    }

    fn activate(&mut self, hit: PanelHit) {
        match hit {
            PanelHit::Close => self.actions.push(Action::ToggleStudio),
            PanelHit::Header(section) => {
                if let Some(i) = StudioSection::ALL.iter().position(|s| *s == section) {
                    self.open[i] = !self.open[i];
                }
            }
        }
    }

    /// Registers a rectangle that can be pointed at.
    fn register(&mut self, hit: PanelHit, rect: Rect) {
        self.next_hits.push((hit, rect));
    }

    /// The smoothed hover amount of `hit`, moved towards where the pointer is.
    fn hover_amount(&mut self, hit: PanelHit) -> f32 {
        let target = if self.hovered == Some(hit) { 1.0 } else { 0.0 };
        let current = self.hover.get(&hit).copied().unwrap_or(0.0);
        let next = if self.animations {
            smooth(current, target, self.dt, HOVER_RATE)
        } else {
            target
        };
        self.hover.insert(hit, next);
        next
    }

    // -- Drawing -----------------------------------------------------------

    /// The background and the line along its left edge.
    fn draw_frame(&self, ui: &mut Ui, t: &Theme, panel: Rect) {
        ui.rect(panel, 0.0, t.surface_solid);
        ui.rect(Rect::new(panel.x, panel.y, 1.0, panel.h), 0.0, t.border);
    }

    /// The title and the close button, over a line.
    fn draw_top(&mut self, ui: &mut Ui, m: &Model, panel: Rect) {
        let t = &m.theme;
        let line_h = ui.measure("Ag", TITLE).1;
        ui.text(
            tr(m.lang, Key::StudioTitle),
            panel.x + PAD,
            panel.y + (TOP_H - line_h) / 2.0,
            TITLE,
            t.text,
        );
        let button = Rect::new(
            panel.x + panel.w - PAD - BUTTON,
            panel.y + (TOP_H - BUTTON) / 2.0,
            BUTTON,
            BUTTON,
        );
        self.register(PanelHit::Close, button);
        let hover = self.hover_amount(PanelHit::Close);
        ui.rect(button, BUTTON / 2.0, t.hover.with_alpha(t.hover.a * hover));
        let (cx, cy) = button.center();
        ui.icon(
            ICON_CLOSE,
            cx,
            cy,
            ICON_SIZE,
            lerp_color(t.text_dim, t.text, hover),
        );
        ui.rect(
            Rect::new(panel.x, panel.y + TOP_H - 1.0, panel.w, 1.0),
            0.0,
            t.border,
        );
    }

    /// The sections, cut off at the list's edges and moved by the scroll.
    fn draw_sections(&mut self, ui: &mut Ui, m: &Model, rows: &[SectionRow], list: Rect) {
        let t = &m.theme;
        let fade = self.openness();
        for row in rows {
            let top = list.y + row.top - self.scroll;
            if top + HEADER_H + row.body < list.y || top > list.y + list.h {
                continue; // out of sight
            }
            ui.set_clip(Some(list));
            let header = Rect::new(list.x, top, list.w, HEADER_H);
            // Only the part of the header on show can be pointed at.
            if let Some(shown) = intersect(header, list) {
                self.register(PanelHit::Header(row.section), shown);
            }
            let hover = self.hover_amount(PanelHit::Header(row.section));
            let wash = Rect::new(
                header.x + 8.0,
                header.y + 3.0,
                header.w - 16.0,
                HEADER_H - 6.0,
            );
            ui.rect(wash, 8.0, t.hover.with_alpha(t.hover.a * hover));
            let open = self.is_open(row.section);
            let chevron = if open {
                ICON_CHEVRON_DOWN
            } else {
                ICON_CHEVRON_RIGHT
            };
            let cy = header.y + HEADER_H / 2.0;
            ui.icon(chevron, header.x + PAD + 6.0, cy, 15.0, t.text_dim);
            let line_h = ui.measure("Ag", SECTION).1;
            ui.text(
                tr(m.lang, row.section.title()),
                header.x + PAD + 26.0,
                cy - line_h / 2.0,
                SECTION,
                t.text,
            );
            if row.body > 0.0 {
                let body = Rect::new(list.x, header.y + HEADER_H, list.w, row.body);
                if let Some(shown) = intersect(body, list) {
                    ui.set_clip(Some(shown));
                    let amount = row.body / row.section.body_height();
                    ui.set_opacity(fade * amount);
                    let note_h = ui.measure("Ag", NOTE).1;
                    ui.text(
                        tr(m.lang, Key::StudioEmpty),
                        body.x + PAD + 26.0,
                        body.y + (row.section.body_height() - note_h) / 2.0,
                        NOTE,
                        t.text_dim,
                    );
                    ui.set_opacity(fade);
                }
                ui.set_clip(Some(list));
            }
            let end = header.y + HEADER_H + row.body;
            ui.rect(Rect::new(list.x, end - 1.0, list.w, 1.0), 0.0, t.border);
        }
    }

    fn is_open(&self, section: StudioSection) -> bool {
        StudioSection::ALL
            .iter()
            .position(|s| *s == section)
            .is_some_and(|i| self.open[i])
    }

    /// A thin bar at the right edge while the list is longer than the room.
    fn draw_scroll_bar(&self, ui: &mut Ui, t: &Theme, list: Rect, content_h: f32) {
        ui.set_clip(None);
        let Some((offset, length)) = scroll::thumb(content_h, list.h, self.scroll) else {
            return;
        };
        let bar = Rect::new(list.x + list.w - 6.0, list.y + offset, 3.0, length);
        ui.rect(bar, 1.5, t.text_dim.with_alpha(0.45));
    }
}

impl Default for StudioPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// The part of `a` that is inside `b`, if any.
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let (left, top) = (a.x.max(b.x), a.y.max(b.y));
    let (right, bottom) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    (left < right && top < bottom).then(|| Rect::new(left, top, right - left, bottom - top))
}

#[cfg(test)]
mod tests;
