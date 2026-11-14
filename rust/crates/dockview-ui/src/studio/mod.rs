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

mod audio;
mod body;
mod draw;
mod inputs;
mod recording;
mod route;
mod scenes;
mod scroll;
mod sections;
mod settings;
mod sources;
mod stats;
mod widgets;

use std::collections::HashMap;

use dockview_domain::stats::Stats;
use dockview_domain::view::{Action, Model};

pub use route::route_pointer;
pub use sections::StudioSection;

use crate::input::Pointer;
use crate::motion::{ease_out_cubic, move_toward, smooth};
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};
use sections::stack;

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
    /// A control inside a section: its position among this frame's items (see
    /// `StudioPanel::item`).
    Item(usize),
    /// A slider inside a section: its position among this frame's sliders.
    Slider(usize),
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
    /// What each control of the sections asks for when clicked, in the order
    /// they were drawn (the previous frame's, and the one being drawn).
    items: Vec<Action>,
    next_items: Vec<Action>,
    /// The sliders of the sections, likewise.
    sliders: Vec<inputs::SliderInfo>,
    next_sliders: Vec<inputs::SliderInfo>,
    /// The part of the list on show, which cuts what is drawn and clicked.
    clip: Rect,
    /// Where the indicator of each segmented control is, by name.
    slide: HashMap<&'static str, f32>,
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
            items: Vec::new(),
            next_items: Vec::new(),
            sliders: Vec::new(),
            next_sliders: Vec::new(),
            clip: Rect::new(0.0, 0.0, 0.0, 0.0),
            slide: HashMap::new(),
            hovered: None,
            pressed: None,
            hover: HashMap::new(),
            actions: Vec::new(),
            area: None,
            dt: 0.0,
        }
    }

    /// Unfolds `section` (for the command line, which starts with one open).
    pub fn unfold(&mut self, section: StudioSection) {
        if let Some(i) = StudioSection::ALL.iter().position(|s| *s == section) {
            self.open[i] = true;
            self.amount[i] = 1.0;
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
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        stats: &Stats,
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
        // Only a section that is open, or still folding, has a body to measure
        // (some of them wrap text, which is not free).
        let mut heights = [0.0; 6];
        for (i, section) in StudioSection::ALL.into_iter().enumerate() {
            if self.open[i] || self.amount[i] > 0.0 {
                heights[i] = section.body_height(ui, m, stats, full_width);
            }
        }
        let (rows, content_h) = stack(&self.amount, &heights);
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
        self.next_items.clear();
        self.next_sliders.clear();
        ui.set_opacity(fade);
        self.draw_frame(ui, t, panel);
        self.draw_top(ui, m, panel);
        self.draw_sections(ui, m, stats, &rows, list);
        self.draw_scroll_bar(ui, t, list, content_h);
        ui.set_clip(None);
        ui.set_opacity(1.0);
        std::mem::swap(&mut self.hits, &mut self.next_hits);
        std::mem::swap(&mut self.items, &mut self.next_items);
        std::mem::swap(&mut self.sliders, &mut self.next_sliders);
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
        // A slider follows the pointer for as long as it is held.
        if let (Some(PanelHit::Slider(i)), Some((x, _))) = (self.pressed, pointer.pos) {
            if pointer.down || pointer.just_pressed {
                self.drag_slider(i, x);
            }
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

    /// Asks for the value of slider `i` that the pointer at `x` points at.
    fn drag_slider(&mut self, i: usize, x: f32) {
        let Some(slider) = self.sliders.get(i).copied() else {
            return;
        };
        let value = slider.value_at(x);
        if (value - slider.value).abs() > f32::EPSILON {
            self.actions.push((slider.make)(value));
            // Until the application answers, the slider shows where it was
            // dragged to (so a fast drag is not asked for twice).
            if let Some(shown) = self.sliders.get_mut(i) {
                shown.value = value;
            }
        }
    }

    fn activate(&mut self, hit: PanelHit) {
        match hit {
            PanelHit::Close => self.actions.push(Action::ToggleStudio),
            PanelHit::Slider(_) => {}
            PanelHit::Item(i) => {
                if let Some(action) = self.items.get(i) {
                    self.actions.push(action.clone());
                }
            }
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
