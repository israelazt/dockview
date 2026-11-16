//! Reusable pieces the pill and the popovers are built from.

use super::*;

impl Controls {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn icon_button(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        hit: Hit,
        icon: char,
        cx: f32,
        cy: f32,
        size: f32,
        active: bool,
    ) {
        let rect = Rect::new(cx - BUTTON / 2.0, cy - BUTTON / 2.0, BUTTON, BUTTON);
        self.register(hit, rect);
        let (hover, press) = self.amounts(hit);
        let strength = hover.max(if active { 1.0 } else { 0.0 });
        let alpha = m.theme.hover.a * (strength + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(rect, BUTTON / 2.0, m.theme.hover.with_alpha(alpha.min(1.0)));
        }
        // The icon dips slightly under the finger.
        ui.icon(icon, cx, cy, size * (1.0 - 0.1 * press), m.theme.text);
    }

    /// The soft highlight under a control while the pointer is over it or
    /// pressing it. `hover_weight` scales the hover part (rows use less than
    /// buttons). Returns the hover and press amounts for the caller to use.
    pub(super) fn wash(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        hit: Hit,
        rect: Rect,
        radius: f32,
        hover_weight: f32,
    ) -> (f32, f32) {
        let (hover, press) = self.amounts(hit);
        let alpha = m.theme.hover.a * (hover * hover_weight + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(rect, radius, m.theme.hover.with_alpha(alpha.min(1.0)));
        }
        (hover, press)
    }

    pub(super) fn draw_slider(&mut self, ui: &mut Ui, m: &Model, bounds: Rect) {
        let t = &m.theme;
        let (_, cy) = bounds.center();
        let track = Rect::new(bounds.x, cy - 2.0, bounds.w, 4.0);
        self.volume_track = track;
        self.register(
            Hit::Volume,
            Rect::new(track.x - 8.0, cy - 12.0, track.w + 16.0, 24.0),
        );

        let value = m.volume.clamp(0.0, 1.0);
        ui.rect(track, 2.0, t.track);
        ui.rect(
            Rect::new(track.x, track.y, track.w * value, track.h),
            2.0,
            t.accent,
        );
        let dragging = self.pressed == Some(Hit::Volume);
        let (grow, _) = self.animate(
            Hit::Volume,
            self.is_hovered(Hit::Volume) || dragging,
            dragging,
        );
        let knob_x = track.x + track.w * value;
        // A soft halo and a slightly larger knob while the slider is in use.
        let halo = 6.0 + 6.0 * grow;
        if grow > 0.01 {
            ui.rect(
                Rect::new(knob_x - halo, cy - halo, halo * 2.0, halo * 2.0),
                halo,
                t.accent.with_alpha(0.22 * grow),
            );
        }
        let r = 6.0 + 1.0 * grow;
        ui.rect(Rect::new(knob_x - r, cy - r, r * 2.0, r * 2.0), r, t.text);
    }
}
