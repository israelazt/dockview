//! The live controls: a floating pill over the video and a settings popover.
//!
//! Immediate mode: every frame [`Controls::frame`] reads the pointer, decides
//! what is hovered/pressed using the *previous* frame's hit rectangles,
//! emits [`Action`]s, and draws. The application applies the actions.

use std::collections::HashMap;
use std::f32::consts::TAU;

use dockview_platform::settings::{
    recording_bytes_per_hour, RecordingQuality, MAX_AUDIO_OFFSET_MS,
};

mod consts;
mod interaction;
mod math;
mod model;
mod pill;
mod popover_replay;
mod popover_settings;
mod popover_sources;
mod scenes;
#[cfg(test)]
mod tests;
mod tooltip;
mod widgets;

use consts::*;
pub use consts::{IDLE_HIDE_SECS, SYNC_STEP_MS, WHEEL_VOLUME_STEP};
use math::*;
pub use model::*;
use scenes::SceneHit;
pub use scenes::{LayerView, SceneAction, ScenesView};
use widgets::*;

use super::input::Pointer;
use super::stats::wrap_text;
use super::text::Weight;
use super::{Rect, TextStyle, Ui};
use crate::i18n::{tr, Key};
use crate::theme::{Color, Theme};

/// Identifies an interactive (or occluding) region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Hit {
    Pill,
    Popover,
    Volume,
    SyncMinus,
    SyncPlus,
    Settings,
    Fullscreen,
    Video(usize),
    /// Pill button that opens the source popover.
    Sources,
    SourceMonitor(usize),
    SourceWindow(usize),
    Audio(usize),
    MicDevice(usize),
    Preset(usize),
    Theme(usize),
    Lang(usize),
    VideoMode(usize),
    Stats,
    /// Pill button that opens the replay popover.
    Replay,
    /// Pill button that saves the replay.
    ReplaySave,
    /// Pill button that starts or stops a recording.
    Record,
    ReplayToggle,
    /// Only an animation key for the switch knob; never registered.
    ReplayKnob,
    ReplayShorter,
    ReplayLonger,
    ReplayPopSave,
    ReplayFolder,
    ReplayChangeFolder,
    Quality(usize),
    MicToggle,
    /// Only an animation key for the microphone switch knob; never registered.
    MicKnob,
    /// Anything in the scenes popover, and the pill's button for it.
    Scenes(SceneHit),
}

impl Hit {
    /// Background regions only block clicks; they are not "buttons".
    fn is_widget(self) -> bool {
        !matches!(self, Hit::Pill | Hit::Popover)
    }

    /// Belongs to the settings popover.
    fn in_popover(self) -> bool {
        matches!(
            self,
            Hit::Popover
                | Hit::Video(_)
                | Hit::SourceMonitor(_)
                | Hit::SourceWindow(_)
                | Hit::Audio(_)
                | Hit::MicDevice(_)
                | Hit::MicToggle
                | Hit::Preset(_)
                | Hit::Theme(_)
                | Hit::Lang(_)
                | Hit::VideoMode(_)
                | Hit::Stats
                | Hit::ReplayToggle
                | Hit::ReplayShorter
                | Hit::ReplayLonger
                | Hit::ReplayPopSave
                | Hit::ReplayFolder
                | Hit::ReplayChangeFolder
                | Hit::Quality(_)
        ) || matches!(self, Hit::Scenes(h) if h.in_popover())
    }
}

/// Which popover is (or was last) open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopoverKind {
    Settings,
    Replay,
    Sources,
    Scenes,
}

pub struct Controls {
    settings_open: bool,
    replay_open: bool,
    sources_open: bool,
    scenes_open: bool,
    /// The scenes popover is showing the list of sources to add.
    scenes_adding: bool,
    popover_kind: PopoverKind,
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
    /// Popover open/close progress, 0.0–1.0 (linear in time).
    popover_t: f32,
    /// Animated position of each segmented control's indicator.
    segments: Option<[f32; 4]>,
    /// The control the pointer is resting on, and for how long (seconds).
    tip_hit: Option<Hit>,
    tip_time: f32,
    /// Animated position of the quality selector in the replay popover.
    quality_slide: Option<f32>,
}

impl Controls {
    /// With `always_visible`, the controls never auto-hide (used by the
    /// interface preview).
    pub fn new(always_visible: bool) -> Self {
        Self {
            settings_open: false,
            replay_open: false,
            sources_open: false,
            scenes_open: false,
            scenes_adding: false,
            popover_kind: PopoverKind::Settings,
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
            popover_t: 0.0,
            segments: None,
            tip_hit: None,
            tip_time: 0.0,
            quality_slide: None,
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

    /// Whether the pointer is over a clickable widget.
    pub fn wants_pointer(&self) -> bool {
        self.is_visible() && self.hovered.is_some_and(Hit::is_widget)
    }

    /// Closes whichever popover is open.
    fn close_popovers(&mut self) {
        self.settings_open = false;
        self.replay_open = false;
        self.sources_open = false;
        self.scenes_open = false;
        self.scenes_adding = false;
    }

    /// Closes the popover; returns whether it was open.
    pub fn close_settings(&mut self) -> bool {
        let was_open = self.popover_target();
        self.close_popovers();
        was_open
    }

    pub fn toggle_settings(&mut self) {
        let was_open = self.settings_open;
        self.close_popovers();
        self.settings_open = !was_open;
        if self.settings_open {
            self.popover_kind = PopoverKind::Settings;
        }
    }

    pub fn toggle_replay(&mut self) {
        let was_open = self.replay_open;
        self.close_popovers();
        self.replay_open = !was_open;
        if self.replay_open {
            self.popover_kind = PopoverKind::Replay;
        }
    }

    pub fn toggle_sources(&mut self) {
        let was_open = self.sources_open;
        self.close_popovers();
        self.sources_open = !was_open;
        if self.sources_open {
            self.popover_kind = PopoverKind::Sources;
        }
    }

    /// Whether the source popover is open (the app refreshes its lists then).
    pub fn sources_open(&self) -> bool {
        self.sources_open
    }

    pub fn replay_open(&self) -> bool {
        self.replay_open
    }

    /// Whether a popover is meant to be showing (as opposed to fading out).
    fn popover_target(&self) -> bool {
        self.settings_open || self.replay_open || self.sources_open || self.scenes_open
    }

    pub fn settings_open(&self) -> bool {
        self.settings_open
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
        // After input, so a click that opens the popover starts animating at once.
        self.update_popover_progress();

        ui.set_opacity(self.visibility);
        self.next_hits.clear();
        let pill = self.draw_pill(ui, model);
        if self.popover_t > 0.0 {
            let eased = ease_out_cubic(self.popover_t);
            ui.set_opacity(self.visibility * eased);
            ui.set_offset(0.0, (1.0 - eased) * POPOVER_RISE);
            match self.popover_kind {
                PopoverKind::Settings => self.draw_popover(ui, model, pill),
                PopoverKind::Replay => self.draw_replay_popover(ui, model, pill),
                PopoverKind::Sources => self.draw_sources_popover(ui, model, pill),
                PopoverKind::Scenes => self.draw_scenes_popover(ui, model, pill),
            }
            ui.set_offset(0.0, 0.0);
        }
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

        let keep = self.always_visible
            || self.idle < IDLE_HIDE_SECS
            || self.popover_target()
            || self.popover_t > 0.0
            || self.pressed.is_some()
            || over_ui;
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

    /// Advances the popover open/close progress and forgets its transient
    /// animation state once it is fully closed.
    fn update_popover_progress(&mut self) {
        let target = if self.popover_target() { 1.0 } else { 0.0 };
        self.popover_t = if self.animations {
            move_toward(self.popover_t, target, self.dt / POPOVER_SECS)
        } else {
            target
        };
        if self.popover_t <= 0.0 && !self.popover_target() {
            self.segments = None;
            self.amounts.retain(|hit, _| !hit.in_popover());
        }
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
        // A popover that is fading out must not receive clicks.
        if hit.in_popover() && !self.popover_target() {
            return;
        }
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
