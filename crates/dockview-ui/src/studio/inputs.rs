//! The widgets of the Studio panel that take more than a click: a slider that
//! is dragged and an on-off switch. They ask for the same [`Action`]s as the
//! pill and the popovers.

use dockview_domain::theme::{Color, Theme};
use dockview_domain::view::Action;

use super::widgets::ROW_H;
use super::{intersect, PanelHit, StudioPanel};
use crate::motion::{lerp_color, smooth};
use crate::{Rect, Ui};

/// Smoothing rate of the knob of a switch.
const KNOB_RATE: f32 = 24.0;
/// How far from the track the slider still takes the pointer, up and down.
const SLIDER_REACH: f32 = 12.0;

/// A slider the panel drew in the last frame: where its track is, what value
/// it shows and what dragging it to a new value asks for.
#[derive(Clone, Copy)]
pub(super) struct SliderInfo {
    pub track: Rect,
    pub value: f32,
    pub make: fn(f32) -> Action,
}

impl SliderInfo {
    /// The value (0 to 1, in whole percent) the pointer at `x` points at.
    pub fn value_at(&self, x: f32) -> f32 {
        let raw = ((x - self.track.x) / self.track.w.max(1.0)).clamp(0.0, 1.0);
        (raw * 100.0).round() / 100.0
    }
}

impl StudioPanel {
    /// A slider from 0 to 1 over `w` pixels starting at `(x, y)` (a row
    /// tall); dragging it asks for `make(value)`. Returns its height.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn slider(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        value: f32,
        make: fn(f32) -> Action,
        x: f32,
        y: f32,
        w: f32,
    ) -> f32 {
        let cy = y + ROW_H / 2.0;
        let track = Rect::new(x, cy - 2.0, w, 4.0);
        let id = self.next_sliders.len();
        self.next_sliders.push(SliderInfo {
            track,
            value: value.clamp(0.0, 1.0),
            make,
        });
        let reach = Rect::new(x - 8.0, cy - SLIDER_REACH, w + 16.0, SLIDER_REACH * 2.0);
        let hit = PanelHit::Slider(id);
        if let Some(shown) = intersect(reach, self.clip) {
            self.register(hit, shown);
        }
        let dragging = self.pressed == Some(hit);
        let grow = self.hover_amount(hit).max(if dragging { 1.0 } else { 0.0 });
        let value = value.clamp(0.0, 1.0);
        ui.rect(track, 2.0, t.track);
        ui.rect(
            Rect::new(track.x, track.y, track.w * value, track.h),
            2.0,
            t.accent,
        );
        let knob_x = track.x + track.w * value;
        let halo = 6.0 + 6.0 * grow;
        if grow > 0.01 {
            ui.rect(
                Rect::new(knob_x - halo, cy - halo, halo * 2.0, halo * 2.0),
                halo,
                t.accent.with_alpha(0.22 * grow),
            );
        }
        let r = 6.0 + grow;
        ui.rect(Rect::new(knob_x - r, cy - r, r * 2.0, r * 2.0), r, t.text);
        ROW_H
    }

    /// An on-off switch in `track`, which the whole `row` toggles by asking
    /// for `action`. `key` names it so its knob can glide when `on` changes.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn switch(
        &mut self,
        ui: &mut Ui,
        t: &Theme,
        key: &'static str,
        on: bool,
        track: Rect,
        row: Rect,
        action: Action,
    ) {
        let hit = self.item(row, action);
        let hover = self.hover_amount(hit);
        ui.rect(row, 10.0, t.hover.with_alpha(t.hover.a * hover * 0.6));

        let target = if on { 1.0 } else { 0.0 };
        let current = self.slide.get(key).copied().unwrap_or(target);
        let amount = if self.animations {
            smooth(current, target, self.dt, KNOB_RATE)
        } else {
            target
        };
        self.slide.insert(key, amount);
        ui.rect(track, track.h / 2.0, lerp_color(t.track, t.accent, amount));
        let r = track.h / 2.0 - 2.0;
        let knob_x = track.x + 2.0 + (track.w - track.h) * amount;
        ui.rect(
            Rect::new(knob_x, track.y + 2.0, r * 2.0, r * 2.0),
            r,
            Color::hex(0xFFFFFF, 1.0),
        );
    }
}
