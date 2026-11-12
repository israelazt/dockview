//! The floating pill: status, volume, sync and the row of buttons.

use super::*;

impl Controls {
    pub(super) fn draw_pill(&mut self, ui: &mut Ui, m: &Model) -> Rect {
        let t = &m.theme;
        let value = BODY.tabular();

        let (status_key, dot) = match m.status {
            StatusKind::Live => (Key::Live, t.live),
            StatusKind::Connecting => (Key::Connecting, t.warn),
            StatusKind::Problem => (Key::Disconnected, t.error),
            StatusKind::Idle => (Key::NoDevice, t.text_dim),
        };
        let status = tr(m.lang, status_key);
        let sync_label = tr(m.lang, Key::Sync);
        let percent = format!("{:.0}%", m.volume * 100.0);
        let sync_value = format!("{:+} ms", m.sync_ms).replace('-', "−");
        let volume_icon = if m.volume <= 0.0 {
            ICON_VOLUME_MUTE
        } else if m.volume < 0.5 {
            ICON_VOLUME_LOW
        } else {
            ICON_VOLUME_HIGH
        };

        // Widths use the widest plausible value so the pill does not resize
        // while numbers change.
        let status_w = ui.measure(status, LABEL).0;
        let res_w = ui
            .measure(&m.resolution, value)
            .0
            .max(ui.measure("1080p · 60 fps", value).0);
        let sync_label_w = ui.measure(sync_label, BODY).0;
        let percent_w = ui.measure("100%", value).0;
        let sync_value_w = ui.measure("−300 ms", value).0;

        let status_block = 8.0 + 8.0 + status_w + 12.0 + res_w;
        let volume_block = ICON_SIZE + 10.0 + SLIDER_WIDTH + 10.0 + percent_w;
        let sync_block = sync_label_w + 10.0 + BUTTON + sync_value_w + BUTTON;
        let buttons_block = BUTTON * 4.0 + 12.0;
        let divider = GAP * 2.0 + 1.0;
        let replay_block = BUTTON * 3.0 + 8.0 + divider;
        let width = PAD_X * 2.0
            + status_block
            + volume_block
            + sync_block
            + replay_block
            + buttons_block
            + divider * 3.0;

        let (screen_w, screen_h) = ui.logical_size();
        let pill = Rect::new(
            ((screen_w - width) / 2.0).round(),
            (screen_h - BOTTOM_MARGIN - PILL_HEIGHT).round(),
            width.round(),
            PILL_HEIGHT,
        );
        self.register(Hit::Pill, pill);
        let cy = pill.y + PILL_HEIGHT / 2.0;

        ui.shadow(pill, PILL_HEIGHT / 2.0, 22.0, t.shadow);
        ui.panel(pill, PILL_HEIGHT / 2.0, t.surface, t.border, 1.0);

        let line_h = ui.measure("Ag", BODY).1;
        let text_y = cy - line_h / 2.0;
        let mut x = pill.x + PAD_X;

        // Status. While connecting, the dot pulses gently.
        let pulse = if m.status == StatusKind::Connecting && self.animations {
            0.55 + 0.45 * (0.5 + 0.5 * (self.time * TAU / PULSE_SECS).sin())
        } else {
            1.0
        };
        ui.rect(
            Rect::new(x, cy - 4.0, 8.0, 8.0),
            4.0,
            dot.with_alpha(dot.a * pulse),
        );
        x += 16.0;
        ui.text(status, x, text_y, LABEL, t.text);
        x += status_w + 12.0;
        if !m.resolution.is_empty() {
            ui.text(&m.resolution, x, text_y, value, t.text_dim);
        }
        x += res_w;
        x = divider_at(ui, t, x, cy);

        // Volume.
        ui.icon(volume_icon, x + ICON_SIZE / 2.0, cy, ICON_SIZE, t.text);
        x += ICON_SIZE + 10.0;
        self.draw_slider(ui, m, Rect::new(x, cy - 6.0, SLIDER_WIDTH, 12.0));
        x += SLIDER_WIDTH + 10.0;
        ui.text(&percent, x, text_y, value, t.text_dim);
        x += percent_w;
        x = divider_at(ui, t, x, cy);

        // Sync.
        ui.text(sync_label, x, text_y, BODY, t.text_dim);
        x += sync_label_w + 10.0;
        self.icon_button(
            ui,
            m,
            Hit::SyncMinus,
            ICON_MINUS,
            x + BUTTON / 2.0,
            cy,
            14.0,
            false,
        );
        x += BUTTON;
        let value_x = x + (sync_value_w - ui.measure(&sync_value, value).0) / 2.0;
        ui.text(&sync_value, value_x, text_y, value, t.text);
        x += sync_value_w;
        self.icon_button(
            ui,
            m,
            Hit::SyncPlus,
            ICON_PLUS,
            x + BUTTON / 2.0,
            cy,
            14.0,
            false,
        );
        x += BUTTON;
        x = divider_at(ui, t, x, cy);

        // Instant replay: its options, and "save the last N".
        let replay_open = self.replay_open;
        self.icon_button(
            ui,
            m,
            Hit::Replay,
            ICON_REPLAY,
            x + BUTTON / 2.0,
            cy,
            ICON_SIZE,
            replay_open,
        );
        if m.replay.is_on() {
            // A small red dot while the buffer is running.
            let pulse = if m.replay.state == ReplayState::Starting && self.animations {
                0.5 + 0.5 * (self.time * TAU / PULSE_SECS).sin().abs()
            } else {
                1.0
            };
            ui.rect(
                Rect::new(x + BUTTON / 2.0 + 3.0, cy - 12.0, 9.0, 9.0),
                4.5,
                t.error.with_alpha(pulse),
            );
        }
        if m.mic.state == MicState::Live {
            // The microphone is open: a red microphone at the button's corner.
            ui.icon(ICON_MIC, x + BUTTON / 2.0 + 10.0, cy + 10.0, 11.0, t.error);
        }
        x += BUTTON + 4.0;
        self.save_button(ui, m, x + BUTTON / 2.0, cy);
        x += BUTTON + 4.0;
        self.record_button(ui, m, x + BUTTON / 2.0, cy);
        x += BUTTON;
        x = divider_at(ui, t, x, cy);

        // Actions.
        let sources_open = self.sources_open;
        self.icon_button(
            ui,
            m,
            Hit::Sources,
            ICON_SOURCE,
            x + BUTTON / 2.0,
            cy,
            ICON_SIZE,
            sources_open,
        );
        x += BUTTON + 4.0;
        self.scenes_button(ui, m, x + BUTTON / 2.0, cy);
        x += BUTTON + 4.0;
        let open = self.settings_open;
        self.icon_button(
            ui,
            m,
            Hit::Settings,
            ICON_SETTINGS,
            x + BUTTON / 2.0,
            cy,
            ICON_SIZE,
            open,
        );
        x += BUTTON + 4.0;
        let fullscreen_icon = if m.fullscreen {
            ICON_EXIT_FULLSCREEN
        } else {
            ICON_FULLSCREEN
        };
        self.icon_button(
            ui,
            m,
            Hit::Fullscreen,
            fullscreen_icon,
            x + BUTTON / 2.0,
            cy,
            ICON_SIZE,
            false,
        );

        pill
    }

    /// The pill's "save clip" button: dimmed while the buffer is off, and
    /// pulsing while a clip is being written.
    pub(super) fn save_button(&mut self, ui: &mut Ui, m: &Model, cx: f32, cy: f32) {
        let t = &m.theme;
        let rect = Rect::new(cx - BUTTON / 2.0, cy - BUTTON / 2.0, BUTTON, BUTTON);
        self.register(Hit::ReplaySave, rect);
        let (hover, press) = self.amounts(Hit::ReplaySave);
        let alpha = t.hover.a * (hover + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(rect, BUTTON / 2.0, t.hover.with_alpha(alpha.min(1.0)));
        }
        let color = if m.replay.is_on() { t.text } else { t.text_dim };
        let pulse = if m.replay.saving && self.animations {
            0.45 + 0.55 * (0.5 + 0.5 * (self.time * TAU / 0.8).sin())
        } else {
            1.0
        };
        ui.icon(
            ICON_SAVE,
            cx,
            cy,
            ICON_SIZE * (1.0 - 0.1 * press),
            color.with_alpha(color.a * pulse),
        );
    }

    /// The pill's record button: a red circle when idle, a red square while recording.
    pub(super) fn record_button(&mut self, ui: &mut Ui, m: &Model, cx: f32, cy: f32) {
        let t = &m.theme;
        let rect = Rect::new(cx - BUTTON / 2.0, cy - BUTTON / 2.0, BUTTON, BUTTON);
        self.register(Hit::Record, rect);
        let (hover, press) = self.amounts(Hit::Record);
        let alpha = t.hover.a * (hover + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(rect, BUTTON / 2.0, t.hover.with_alpha(alpha.min(1.0)));
        }
        let scale = 1.0 - 0.1 * press;
        match m.rec.state {
            RecState::Idle => {
                let size = 14.0 * scale;
                ui.rect(
                    Rect::new(cx - size / 2.0, cy - size / 2.0, size, size),
                    size / 2.0,
                    t.error,
                );
            }
            RecState::Starting | RecState::Recording => {
                let pulse = if m.rec.state == RecState::Starting && self.animations {
                    0.5 + 0.5 * (self.time * TAU / PULSE_SECS).sin().abs()
                } else {
                    1.0
                };
                let size = 13.0 * scale;
                ui.rect(
                    Rect::new(cx - size / 2.0, cy - size / 2.0, size, size),
                    3.0,
                    t.error.with_alpha(pulse),
                );
            }
        }
    }
}
