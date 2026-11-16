//! The live controls: a floating bar over the video with the buttons used
//! while watching (clip, record, screenshot, volume, microphone, full screen).
//!
//! Immediate mode: every frame [`Controls::frame`] reads the pointer, decides
//! what is hovered/pressed using the *previous* frame's hit rectangles,
//! emits [`Action`]s, and draws. The application applies the actions.

use std::collections::HashMap;
use std::f32::consts::TAU;

use dockview_domain::settings::MAX_AUDIO_OFFSET_MS;

mod buttons;
mod consts;
mod interaction;
mod math;
mod pill;
#[cfg(test)]
mod tests;
mod tooltip;
mod widgets;

use consts::*;
pub use consts::{IDLE_HIDE_SECS, SYNC_STEP_MS, WHEEL_VOLUME_STEP};
use dockview_domain::view::*;
use math::*;

use super::input::Pointer;
use super::stats::wrap_text;
use super::text::Weight;
use super::{Rect, TextStyle, Ui};
use dockview_domain::i18n::{tr, Key};
use dockview_domain::theme::{Color, Theme};

/// Identifies an interactive (or occluding) region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Hit {
    Pill,
    Volume,
    SyncMinus,
    SyncPlus,
    Fullscreen,
    /// Bar button that saves the replay.
    ReplaySave,
    /// Bar button that starts or stops a recording.
    Record,
    /// Bar button that takes a screenshot.
    Capture,
    /// Bar button that turns the microphone on or off.
    Mic,
    /// Bar button that shows or hides the readouts.
    Hud,
}

impl Hit {
    /// Background regions only block clicks; they are not "buttons".
    fn is_widget(self) -> bool {
        self != Hit::Pill
    }
}

pub struct Controls {
    always_visible: bool,
    visibility: f32,
    idle: f32,
    last_pos: Option<(f32, f32)>,
    hovered: Option<Hit>,
    pressed: Option<Hit>,
    /// Rectangles registered by the previous frame; later entries are on top.
    hits: Vec<(Hit, Rect)>,
    next_hits: Vec<(Hit, Rect)>,
    volume_track: Rect,
    actions: Vec<Action>,
    animations: bool,
    /// Seconds since creation; drives the looping pulse.
    time: f32,
    /// Duration of the frame being processed.
    dt: f32,
    /// Smoothed (hover, pressed) amounts per widget, 0.0–1.0.
    amounts: HashMap<Hit, (f32, f32)>,
    /// The control the pointer is resting on, and for how long (seconds).
    tip_hit: Option<Hit>,
    tip_time: f32,
}

impl Controls {
    /// With `always_visible`, the controls never auto-hide (used by the
    /// interface preview).
    pub fn new(always_visible: bool) -> Self {
        Self {
            always_visible,
            visibility: if always_visible { 1.0 } else { 0.0 },
            idle: IDLE_HIDE_SECS,
            last_pos: None,
            hovered: None,
            pressed: None,
            hits: Vec::new(),
            next_hits: Vec::new(),
            volume_track: Rect::new(0.0, 0.0, 1.0, 1.0),
            actions: Vec::new(),
            animations: true,
            time: 0.0,
            dt: 0.0,
            amounts: HashMap::new(),
            tip_hit: None,
            tip_time: 0.0,
        }
    }

    /// Turns motion on or off (off snaps every transition, honouring the
    /// Windows "show animations" setting).
    pub fn set_animations(&mut self, on: bool) {
        self.animations = on;
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// Whether anything is currently drawn.
    pub fn is_visible(&self) -> bool {
        self.visibility > 0.01
    }

    /// Whether the pointer is over the pill (a button or its background), so
    /// that whatever is under the pill must not get it.
    pub fn over_controls(&self) -> bool {
        self.is_visible() && self.hovered.is_some()
    }

    /// Whether the pointer is over a clickable widget.
    pub fn wants_pointer(&self) -> bool {
        self.is_visible() && self.hovered.is_some_and(Hit::is_widget)
    }

    /// Runs one frame: input, then drawing.
    pub fn frame(&mut self, ui: &mut Ui, model: &Model, pointer: &Pointer, dt: f32) {
        self.dt = dt;
        self.time = (self.time + dt) % 3600.0;
        self.update_visibility(pointer, dt);
        if self.visibility <= 0.0 {
            self.hits.clear();
            self.hovered = None;
            self.pressed = None;
            return;
        }

        self.hovered = self.hit_at(pointer.pos);
        self.update_tooltip_timer(pointer);
        self.interact(model, pointer);

        ui.set_opacity(self.visibility);
        self.next_hits.clear();
        self.draw_pill(ui, model);
        ui.set_opacity(self.visibility);
        self.draw_tooltip(ui, model);
        std::mem::swap(&mut self.hits, &mut self.next_hits);
        ui.set_opacity(1.0);
    }

    // --------------------------------------------------------------- tooltips

    // ------------------------------------------------------------ visibility

    fn update_visibility(&mut self, pointer: &Pointer, dt: f32) {
        let moved = pointer.pos.is_some() && pointer.pos != self.last_pos;
        self.last_pos = pointer.pos;
        let over_ui = self.hit_at(pointer.pos).is_some();

        if moved || pointer.just_pressed || pointer.wheel != 0.0 {
            self.idle = 0.0;
        } else {
            self.idle += dt;
        }

        let keep =
            self.always_visible || self.idle < IDLE_HIDE_SECS || self.pressed.is_some() || over_ui;
        self.visibility = step_visibility(self.visibility, keep, dt);
    }

    // ----------------------------------------------------------------- input

    fn hit_at(&self, pos: Option<(f32, f32)>) -> Option<Hit> {
        let (x, y) = pos?;
        self.hits
            .iter()
            .rev()
            .find(|(_, rect)| rect.contains(x, y))
            .map(|(hit, _)| *hit)
    }

    /// Eases `current` towards `target`; snaps when animations are off.
    fn approach(&self, current: f32, target: f32, rate: f32) -> f32 {
        if self.animations {
            smooth(current, target, self.dt, rate)
        } else {
            target
        }
    }

    /// Updates and returns the smoothed (hover, pressed) amounts of `hit`.
    fn animate(&mut self, hit: Hit, hover: bool, press: bool) -> (f32, f32) {
        let (h, p) = self.amounts.get(&hit).copied().unwrap_or((0.0, 0.0));
        let h = self.approach(h, if hover { 1.0 } else { 0.0 }, HOVER_RATE);
        let p = self.approach(p, if press { 1.0 } else { 0.0 }, PRESS_RATE);
        self.amounts.insert(hit, (h, p));
        (h, p)
    }

    fn amounts(&mut self, hit: Hit) -> (f32, f32) {
        let (hover, press) = (self.is_hovered(hit), self.is_pressed(hit));
        self.animate(hit, hover, press)
    }

    fn is_hovered(&self, hit: Hit) -> bool {
        self.hovered == Some(hit)
    }

    fn is_pressed(&self, hit: Hit) -> bool {
        self.pressed == Some(hit) && self.hovered == Some(hit)
    }

    fn register(&mut self, hit: Hit, rect: Rect) {
        self.next_hits.push((hit, rect));
    }

    // --------------------------------------------------------------- drawing

    #[cfg(test)]
    pub(super) fn rect_of(&self, hit: Hit) -> Rect {
        self.hits
            .iter()
            .rev()
            .find(|(h, _)| *h == hit)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("{hit:?} was not drawn"))
    }
}
