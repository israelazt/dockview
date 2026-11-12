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

    /// An on/off switch. The knob glides when `on` changes.
    pub(super) fn switch(&mut self, ui: &mut Ui, m: &Model, track: Rect, on: bool, knob: Hit) {
        let t = &m.theme;
        let (amount, _) = self.animate(knob, on, false);
        let fill = lerp_color(t.track, t.accent, amount);
        ui.rect(track, track.h / 2.0, fill);
        let r = track.h / 2.0 - 2.0;
        let knob_x = track.x + 2.0 + (track.w - track.h) * amount;
        ui.rect(
            Rect::new(knob_x, track.y + 2.0, r * 2.0, r * 2.0),
            r,
            Color::hex(0xFFFFFF, 1.0),
        );
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

    /// A titled list of choices (at most `limit` rows), one of them ticked.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn list_section(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        title: Key,
        devices: &[String],
        selected: Option<usize>,
        hit: fn(usize) -> Hit,
        x: f32,
        mut y: f32,
        w: f32,
        limit: usize,
    ) -> f32 {
        let t = &m.theme;
        ui.text(tr(m.lang, title), x, y, SECTION, t.text_dim);
        y += LABEL_HEIGHT;

        if devices.is_empty() {
            let line_h = ui.measure("Ag", BODY).1;
            ui.text(
                tr(m.lang, Key::NoDevice),
                x + 10.0,
                y + (ROW_HEIGHT - line_h) / 2.0,
                BODY,
                t.text_dim,
            );
            return y + ROW_HEIGHT;
        }

        for (i, name) in devices.iter().take(limit).enumerate() {
            let row = Rect::new(x, y, w, ROW_HEIGHT);
            self.register(hit(i), row);
            let is_selected = selected == Some(i);
            let (hover, press) = self.amounts(hit(i));
            let alpha = t.hover.a * (hover + 0.9 * press);
            if alpha > 0.002 {
                ui.rect(row, 9.0, t.hover.with_alpha(alpha.min(1.0)));
            }
            let label = fit_text(ui, name, BODY, w - 20.0 - 28.0);
            let line_h = ui.measure("Ag", BODY).1;
            let color = if is_selected { t.text } else { t.text_dim };
            ui.text(
                &label,
                x + 10.0,
                y + (ROW_HEIGHT - line_h) / 2.0,
                BODY,
                color,
            );
            if is_selected {
                ui.icon(
                    ICON_CHECK,
                    x + w - 18.0,
                    y + ROW_HEIGHT / 2.0,
                    15.0,
                    t.accent,
                );
            }
            y += ROW_HEIGHT;
        }
        y
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn segmented_section(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        title: Key,
        labels: &[&str],
        selected: usize,
        position: f32,
        hit: fn(usize) -> Hit,
        x: f32,
        mut y: f32,
        w: f32,
    ) -> f32 {
        let t = &m.theme;
        ui.text(tr(m.lang, title), x, y, SECTION, t.text_dim);
        y += LABEL_HEIGHT;

        let track = Rect::new(x, y, w, ROW_HEIGHT);
        ui.rect(track, 11.0, t.segment_track);
        let segment_w = (w - 6.0) / labels.len().max(1) as f32;
        let style = TextStyle::new(12.5, Weight::Medium);
        let line_h = ui.measure("Ag", style).1;

        // The active indicator glides between segments.
        let indicator = Rect::new(
            x + 3.0 + segment_w * position,
            y + 3.0,
            segment_w,
            ROW_HEIGHT - 6.0,
        );
        ui.rect(indicator, 8.0, t.segment_active);

        for (i, label) in labels.iter().enumerate() {
            let segment = Rect::new(
                x + 3.0 + segment_w * i as f32,
                y + 3.0,
                segment_w,
                ROW_HEIGHT - 6.0,
            );
            self.register(hit(i), segment);
            let (hover, press) = self.amounts(hit(i));
            let alpha = t.hover.a * (hover + 0.9 * press);
            if i != selected && alpha > 0.002 {
                ui.rect(segment, 8.0, t.hover.with_alpha(alpha.min(1.0)));
            }
            let color = if i == selected { t.text } else { t.text_dim };
            let text = fit_text(ui, label, style, segment_w - 8.0);
            let text_w = ui.measure(&text, style).0;
            ui.text(
                &text,
                segment.x + (segment_w - text_w) / 2.0,
                y + (ROW_HEIGHT - line_h) / 2.0,
                style,
                color,
            );
        }
        y + ROW_HEIGHT
    }
}

pub(super) fn divider_at(ui: &mut Ui, theme: &Theme, x: f32, cy: f32) -> f32 {
    ui.rect(Rect::new(x + GAP, cy - 10.0, 1.0, 20.0), 0.5, theme.track);
    x + GAP * 2.0 + 1.0
}

/// Shortens `text` with an ellipsis so it fits in `max_w` logical pixels.
pub(super) fn fit_text(ui: &mut Ui, text: &str, style: TextStyle, max_w: f32) -> String {
    if ui.measure(text, style).0 <= max_w {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    for keep in (1..chars.len()).rev() {
        let candidate: String = chars[..keep]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_owned()
            + "…";
        if ui.measure(&candidate, style).0 <= max_w {
            return candidate;
        }
    }
    "…".to_owned()
}
