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
        // The button that opened a popover needs no explanation any more.
        let opened_it = match hit {
            Hit::Replay => self.replay_open,
            Hit::Settings => self.settings_open,
            Hit::Sources => self.sources_open,
            Hit::Scenes(SceneHit::Button) => self.scenes_open,
            _ => false,
        };
        if opened_it {
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

        // Items inside a popover get their tooltip to the left of it (so it
        // does not cover the neighbouring rows); the pill's buttons get it above.
        let popover = self
            .next_hits
            .iter()
            .rev()
            .find(|(h, _)| *h == Hit::Popover)
            .map(|(_, r)| *r);
        let beside = popover.filter(|p| hit.in_popover() && p.x - w - 10.0 >= 8.0);
        let (x, y) = match beside {
            Some(p) => (p.x - w - 10.0, anchor.y + anchor.h / 2.0 - h / 2.0),
            None => (anchor.x + anchor.w / 2.0 - w / 2.0, anchor.y - h - 8.0),
        };
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
        Hit::Sources => Key::TipSources,
        Hit::Scenes(h) => scenes::tooltip_key(h, m)?,
        Hit::Replay => Key::TipReplay,
        Hit::ReplaySave | Hit::ReplayPopSave if r.is_on() => Key::TipSave,
        Hit::ReplaySave | Hit::ReplayPopSave => Key::TipSaveOff,
        Hit::Record => match m.rec.state {
            RecState::Idle => Key::TipRecord,
            RecState::Starting => Key::TipRecordCancel,
            RecState::Recording => Key::TipRecordStop,
        },
        Hit::Settings => Key::TipSettings,
        Hit::Fullscreen if m.fullscreen => Key::TipExitFullscreen,
        Hit::Fullscreen => Key::TipFullscreen,
        Hit::Volume => Key::TipVolume,
        Hit::SyncMinus => Key::TipSyncEarlier,
        Hit::SyncPlus => Key::TipSyncLater,
        Hit::ReplayToggle => Key::TipReplayToggle,
        Hit::ReplayShorter => Key::TipReplayShorter,
        Hit::ReplayLonger => Key::TipReplayLonger,
        Hit::MicToggle => Key::TipMicToggle,
        Hit::ReplayFolder => Key::TipFolderOpen,
        Hit::ReplayChangeFolder => Key::TipFolderChange,
        Hit::Quality(0) => Key::TipQualityNormal,
        Hit::Quality(_) => Key::TipQualityHigh,
        Hit::VideoMode(0) => Key::TipVideoAuto,
        Hit::VideoMode(_) => Key::TipVideoCompressed,
        Hit::Stats => Key::TipStats,
        Hit::Preset(0) => Key::TipPresetPerformance,
        Hit::Preset(1) => Key::TipPresetBalanced,
        Hit::Preset(_) => Key::TipPresetQuality,
        _ => return None,
    };
    let hotkey = match hit {
        Hit::Record => &r.record_hotkey,
        _ => &r.hotkey,
    };
    Some(
        tr(m.lang, key)
            .replace("{seconds}", &crate::i18n::duration_label(r.keep_seconds))
            .replace("{hotkey}", hotkey),
    )
}
