//! The wide buttons of the bar: save a clip, record and take a screenshot.

use dockview_domain::design::RADIUS_CHIP;

use super::pill::{BarMetrics, Row};
use super::*;

/// Width of a hotkey chip for `text`.
pub(super) fn chip_width(ui: &mut Ui, text: &str) -> f32 {
    ui.measure(text, CHIP_TEXT).0 + 12.0
}

/// The words in the record button: the time while recording, else its name.
pub(super) fn record_label(m: &Model) -> String {
    match m.rec.state {
        RecState::Recording => clock(m.rec.seconds),
        _ => tr(m.lang, Key::BarRecord).to_string(),
    }
}

/// `m:ss`, or `h:mm:ss` from an hour.
pub(super) fn clock(seconds: f32) -> String {
    let total = seconds.max(0.0) as u32;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

impl Controls {
    /// A wide pill-shaped button at `x`, `w` wide on the bar's centre line;
    /// returns its rectangle.
    fn wide_button(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        hit: Hit,
        slot: (f32, f32, f32),
        fill: Color,
    ) -> Rect {
        let (x, w, cy) = slot;
        let rect = Rect::new(x, cy - ITEM_H / 2.0, w, ITEM_H);
        self.register(hit, rect);
        ui.rect(rect, ITEM_H / 2.0, fill);
        self.wash(ui, m, hit, rect, ITEM_H / 2.0, 1.0);
        rect
    }

    /// A hotkey in a small rounded chip at `x`; returns where it ends.
    fn draw_chip(
        &self,
        ui: &mut Ui,
        text: &str,
        x: f32,
        cy: f32,
        fill: Color,
        color: Color,
    ) -> f32 {
        let w = chip_width(ui, text);
        let h = ui.measure(text, CHIP_TEXT).1 + 6.0;
        ui.rect(Rect::new(x, cy - h / 2.0, w, h), RADIUS_CHIP, fill);
        ui.text(text, x + 6.0, cy - (h - 6.0) / 2.0, CHIP_TEXT, color);
        x + w
    }

    /// Save the last seconds as a clip: the main button, in the brand colour
    /// while the replay runs and grey when it is off; it pulses while writing.
    pub(super) fn save_button(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        met: &BarMetrics,
        row: Row,
        x: f32,
    ) -> f32 {
        let t = &m.theme;
        let on = m.replay.is_on();
        let pulse = if m.replay.saving && self.animations {
            0.55 + 0.45 * (0.5 + 0.5 * (self.time * TAU / 0.8).sin())
        } else {
            1.0
        };
        let (fill, fg) = if on {
            (t.brand.with_alpha(pulse), t.on_brand)
        } else {
            (t.segment_track, t.text_dim)
        };
        let rect = self.wide_button(ui, m, Hit::ReplaySave, (x, met.save_w, row.cy), fill);
        let (_, press) = self.amounts(Hit::ReplaySave);
        let mut cx = rect.x + 14.0;
        ui.icon(
            ICON_SAVE,
            cx + 7.5,
            row.cy,
            (ICON_SIZE - 2.0) * (1.0 - 0.1 * press),
            fg,
        );
        cx += ICON_SIZE - 2.0 + 8.0;
        let label = tr(m.lang, Key::BarSave);
        ui.text(label, cx, row.text_y, BUTTON_TEXT, fg);
        cx += ui.measure(label, BUTTON_TEXT).0;
        if met.show_chips && !m.replay.hotkey.is_empty() {
            self.draw_chip(
                ui,
                &m.replay.hotkey,
                cx + GAP,
                row.cy,
                fg.with_alpha(0.18),
                fg,
            );
        }
        rect.x + rect.w
    }

    /// Record: a red circle when idle, a red square while recording (with the
    /// time recorded).
    pub(super) fn record_button(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        met: &BarMetrics,
        row: Row,
        x: f32,
    ) -> f32 {
        let t = &m.theme;
        let rect = self.wide_button(
            ui,
            m,
            Hit::Record,
            (x, met.record_w, row.cy),
            t.segment_track,
        );
        let (_, press) = self.amounts(Hit::Record);
        let scale = 1.0 - 0.1 * press;
        let mut cx = rect.x + 14.0;
        let (dx, dy) = (cx + 6.0, row.cy);
        match m.rec.state {
            RecState::Idle => {
                let size = 12.0 * scale;
                ui.rect(
                    Rect::new(dx - size / 2.0, dy - size / 2.0, size, size),
                    size / 2.0,
                    t.error,
                );
            }
            RecState::Starting | RecState::Recording => {
                let pulse = if m.rec.state == RecState::Starting && self.animations {
                    self.blink()
                } else {
                    1.0
                };
                let size = 11.0 * scale;
                ui.rect(
                    Rect::new(dx - size / 2.0, dy - size / 2.0, size, size),
                    3.0,
                    t.error.with_alpha(pulse),
                );
            }
        }
        cx += 12.0 + 8.0;
        let label = record_label(m);
        let style = if m.rec.state == RecState::Recording {
            BUTTON_TEXT.tabular()
        } else {
            BUTTON_TEXT
        };
        ui.text(&label, cx, row.text_y, style, t.text);
        if met.show_chips && !m.replay.record_hotkey.is_empty() {
            // The label may be shorter than its room: the chip keeps its place.
            let chip_w = chip_width(ui, &m.replay.record_hotkey);
            let chip_x = rect.x + rect.w - 14.0 - chip_w;
            self.draw_chip(
                ui,
                &m.replay.record_hotkey,
                chip_x,
                row.cy,
                t.track,
                t.text_dim,
            );
        }
        rect.x + rect.w
    }

    /// A screenshot, in the mode chosen in the settings.
    pub(super) fn capture_button(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        met: &BarMetrics,
        row: Row,
        x: f32,
    ) -> f32 {
        let t = &m.theme;
        let rect = self.wide_button(
            ui,
            m,
            Hit::Capture,
            (x, met.capture_w, row.cy),
            t.segment_track,
        );
        let (_, press) = self.amounts(Hit::Capture);
        let mut cx = rect.x + 14.0;
        ui.icon(
            ICON_CAMERA,
            cx + 7.5,
            row.cy,
            (ICON_SIZE - 2.0) * (1.0 - 0.1 * press),
            t.text,
        );
        cx += ICON_SIZE - 2.0 + 8.0;
        ui.text(
            tr(m.lang, Key::BarCapture),
            cx,
            row.text_y,
            BUTTON_TEXT,
            t.text,
        );
        rect.x + rect.w
    }
}
