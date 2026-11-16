//! Hover tooltips: when they show, where they go and what they say.

use super::*;

impl Controls {
    /// Counts how long the pointer has rested on the same control. Pressing
    /// (or dragging) hides the tooltip.
    pub(super) fn update_tooltip_timer(&mut self, pointer: &Pointer) {
        let hovered = self.hovered.filter(|h| h.is_widget());
        if hovered != self.tip_hit || pointer.down {
            self.tip_hit = hovered;
            self.tip_time = 0.0;
        } else {
            self.tip_time += self.dt;
        }
    }

    /// Draws the tooltip of the control under the pointer, once it has rested
    /// there long enough.
    pub(super) fn draw_tooltip(&mut self, ui: &mut Ui, m: &Model) {
        let Some(hit) = self.tip_hit else {
            return;
        };
        if self.tip_time < TOOLTIP_DELAY_SECS {
            return;
        }
        let Some(text) = tooltip_for(hit, m) else {
            return;
        };
        let Some(anchor) = self
            .next_hits
            .iter()
            .rev()
            .find(|(h, _)| *h == hit)
            .map(|(_, r)| *r)
        else {
            return;
        };
        let t = &m.theme;
        let style = TextStyle::new(12.5, Weight::Medium);
        let lines = wrap_text(ui, &text, style, TOOLTIP_MAX_WIDTH);
        let text_w = lines
            .iter()
            .map(|l| ui.measure(l, style).0)
            .fold(0.0f32, f32::max);
        let line_h = 17.0;
        let (w, h) = (text_w + 20.0, lines.len() as f32 * line_h + 14.0);
        let (screen_w, _) = ui.logical_size();

        // The tooltip goes above the pill's button.
        let x = anchor.x + anchor.w / 2.0 - w / 2.0;
        let y = anchor.y - h - 8.0;
        let x = x.clamp(8.0, (screen_w - w - 8.0).max(8.0)).round();
        let y = y.max(8.0).round();

        let fade = if self.animations {
            ((self.tip_time - TOOLTIP_DELAY_SECS) / TOOLTIP_FADE_SECS).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let rect = Rect::new(x, y, w.round(), h);
        ui.set_opacity(self.visibility * fade);
        ui.shadow(rect, 10.0, 14.0, t.shadow);
        ui.panel(rect, 10.0, t.surface, t.border, 1.0);
        for (i, line) in lines.iter().enumerate() {
            ui.text(line, x + 10.0, y + 7.0 + i as f32 * line_h, style, t.text);
        }
        ui.set_opacity(self.visibility);
    }
}

/// What a control does, for its tooltip. `None` for backgrounds and for rows
/// whose label already says everything (a device name, a window title).
pub(super) fn tooltip_for(hit: Hit, m: &Model) -> Option<String> {
    let r = &m.replay;
    let key = match hit {
        Hit::ReplaySave if r.is_on() => Key::TipSave,
        Hit::ReplaySave => Key::TipSaveOff,
        Hit::Record => match m.rec.state {
            RecState::Idle => Key::TipRecord,
            RecState::Starting => Key::TipRecordCancel,
            RecState::Recording => Key::TipRecordStop,
        },
        Hit::Capture => Key::TipCapture,
        Hit::Mic if m.mic.state == MicState::Live => Key::TipMicOff,
        Hit::Mic => Key::TipMicOn,
        Hit::Hud if m.hud => Key::TipHudHide,
        Hit::Hud => Key::TipHudShow,
        Hit::Fullscreen if m.fullscreen => Key::TipExitFullscreen,
        Hit::Fullscreen => Key::TipFullscreen,
        Hit::Volume => Key::TipVolume,
        _ => return None,
    };
    let hotkey = match hit {
        Hit::Record => &r.record_hotkey,
        _ => &r.hotkey,
    };
    Some(
        tr(m.lang, key)
            .replace(
                "{seconds}",
                &dockview_domain::i18n::duration_label(r.keep_seconds),
            )
            .replace("{hotkey}", hotkey),
    )
}
