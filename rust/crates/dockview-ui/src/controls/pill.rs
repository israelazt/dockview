//! The floating pill: status, volume, sync and the row of buttons.

use super::*;

/// The widths the pill's parts need. They use the widest plausible value so
/// the pill does not resize while numbers change.
struct PillMetrics {
    status_w: f32,
    res_w: f32,
    sync_label_w: f32,
    percent_w: f32,
    sync_value_w: f32,
    /// The whole pill.
    width: f32,
    /// Whether the resolution text and the sync block are drawn. They are left
    /// out, in that order, when the pill does not fit the room it has.
    show_res: bool,
    show_sync: bool,
}

/// Where a group of controls sits vertically.
#[derive(Clone, Copy)]
struct Row {
    /// Centre line of the pill.
    cy: f32,
    /// Top of a line of text centred on it.
    text_y: f32,
}

fn pill_metrics(ui: &mut Ui, m: &Model, room: f32) -> PillMetrics {
    let value = BODY.tabular();
    let status_w = ui.measure(status_text(m), LABEL).0;
    let res_w = ui
        .measure(&m.resolution, value)
        .0
        .max(ui.measure("1080p · 60 fps", value).0);
    let sync_label_w = ui.measure(tr(m.lang, Key::Sync), BODY).0;
    let percent_w = ui.measure("100%", value).0;
    let sync_value_w = ui.measure("−300 ms", value).0;

    let status_full = 8.0 + 8.0 + status_w + 12.0 + res_w;
    let status_short = 8.0 + 8.0 + status_w;
    let volume_block = ICON_SIZE + 10.0 + SLIDER_WIDTH + 10.0 + percent_w;
    let sync_block = sync_label_w + 10.0 + BUTTON + sync_value_w + BUTTON;
    let buttons_block = BUTTON * 5.0 + 16.0;
    let divider = GAP * 2.0 + 1.0;
    let replay_block = BUTTON * 3.0 + 8.0 + divider;
    let width_of = |status: f32, with_sync: bool| {
        PAD_X * 2.0
            + status
            + volume_block
            + if with_sync { sync_block + divider } else { 0.0 }
            + replay_block
            + buttons_block
            + divider * 3.0
    };
    // Too wide for the room: drop the resolution, then the sync block.
    let (show_res, show_sync) = if width_of(status_full, true) <= room {
        (true, true)
    } else if width_of(status_short, true) <= room {
        (false, true)
    } else {
        (false, false)
    };
    let width = width_of(if show_res { status_full } else { status_short }, show_sync);
    PillMetrics {
        status_w,
        res_w,
        sync_label_w,
        percent_w,
        sync_value_w,
        width,
        show_res,
        show_sync,
    }
}

/// The word next to the status dot.
fn status_text(m: &Model) -> &'static str {
    tr(
        m.lang,
        match m.status {
            StatusKind::Live => Key::Live,
            StatusKind::Connecting => Key::Connecting,
            StatusKind::Problem => Key::Disconnected,
            StatusKind::Idle => Key::NoDevice,
        },
    )
}

/// The volume icon for a level: muted, low or high.
fn volume_icon(volume: f32) -> char {
    if volume <= 0.0 {
        ICON_VOLUME_MUTE
    } else if volume < 0.5 {
        ICON_VOLUME_LOW
    } else {
        ICON_VOLUME_HIGH
    }
}

impl Controls {
    pub(super) fn draw_pill(&mut self, ui: &mut Ui, m: &Model) -> Rect {
        let t = &m.theme;
        let (screen_w, screen_h) = ui.logical_size();
        let metrics = pill_metrics(ui, m, screen_w - 16.0);

        let pill = Rect::new(
            ((screen_w - metrics.width) / 2.0).round().max(8.0),
            (screen_h - BOTTOM_MARGIN - PILL_HEIGHT).round(),
            metrics.width.round(),
            PILL_HEIGHT,
        );
        self.register(Hit::Pill, pill);
        let cy = pill.y + PILL_HEIGHT / 2.0;

        ui.shadow(pill, PILL_HEIGHT / 2.0, 22.0, t.shadow);
        ui.panel(pill, PILL_HEIGHT / 2.0, t.surface, t.border, 1.0);

        let line_h = ui.measure("Ag", BODY).1;
        let row = Row {
            cy,
            text_y: cy - line_h / 2.0,
        };
        let mut x = pill.x + PAD_X;
        x = self.pill_status(ui, m, &metrics, row, x);
        x = divider_at(ui, t, x, cy);
        x = self.pill_volume(ui, m, &metrics, row, x);
        x = divider_at(ui, t, x, cy);
        if metrics.show_sync {
            x = self.pill_sync(ui, m, &metrics, row, x);
            x = divider_at(ui, t, x, cy);
        }
        x = self.pill_replay(ui, m, row, x);
        x = divider_at(ui, t, x, cy);
        self.pill_actions(ui, m, row, x);
        pill
    }

    /// Status. While connecting, the dot pulses gently.
    fn pill_status(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        met: &PillMetrics,
        row: Row,
        mut x: f32,
    ) -> f32 {
        let t = &m.theme;
        let dot = match m.status {
            StatusKind::Live => t.live,
            StatusKind::Connecting => t.warn,
            StatusKind::Problem => t.error,
            StatusKind::Idle => t.text_dim,
        };
        let pulse = if m.status == StatusKind::Connecting && self.animations {
            0.55 + 0.45 * (0.5 + 0.5 * (self.time * TAU / PULSE_SECS).sin())
        } else {
            1.0
        };
        ui.rect(
            Rect::new(x, row.cy - 4.0, 8.0, 8.0),
            4.0,
            dot.with_alpha(dot.a * pulse),
        );
        x += 16.0;
        ui.text(status_text(m), x, row.text_y, LABEL, t.text);
        x += met.status_w;
        if !met.show_res {
            return x;
        }
        x += 12.0;
        if !m.resolution.is_empty() {
            ui.text(&m.resolution, x, row.text_y, BODY.tabular(), t.text_dim);
        }
        x + met.res_w
    }

    fn pill_volume(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        met: &PillMetrics,
        row: Row,
        mut x: f32,
    ) -> f32 {
        let t = &m.theme;
        ui.icon(
            volume_icon(m.volume),
            x + ICON_SIZE / 2.0,
            row.cy,
            ICON_SIZE,
            t.text,
        );
        x += ICON_SIZE + 10.0;
        self.draw_slider(ui, m, Rect::new(x, row.cy - 6.0, SLIDER_WIDTH, 12.0));
        x += SLIDER_WIDTH + 10.0;
        let percent = format!("{:.0}%", m.volume * 100.0);
        ui.text(&percent, x, row.text_y, BODY.tabular(), t.text_dim);
        x + met.percent_w
    }

    fn pill_sync(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        met: &PillMetrics,
        row: Row,
        mut x: f32,
    ) -> f32 {
        let t = &m.theme;
        let value = BODY.tabular();
        ui.text(tr(m.lang, Key::Sync), x, row.text_y, BODY, t.text_dim);
        x += met.sync_label_w + 10.0;
        self.icon_button(
            ui,
            m,
            Hit::SyncMinus,
            ICON_MINUS,
            x + BUTTON / 2.0,
            row.cy,
            14.0,
            false,
        );
        x += BUTTON;
        let sync_value = format!("{:+} ms", m.sync_ms).replace('-', "−");
        let value_x = x + (met.sync_value_w - ui.measure(&sync_value, value).0) / 2.0;
        ui.text(&sync_value, value_x, row.text_y, value, t.text);
        x += met.sync_value_w;
        self.icon_button(
            ui,
            m,
            Hit::SyncPlus,
            ICON_PLUS,
            x + BUTTON / 2.0,
            row.cy,
            14.0,
            false,
        );
        x + BUTTON
    }

    /// Instant replay: its options, and "save the last N", and record.
    fn pill_replay(&mut self, ui: &mut Ui, m: &Model, row: Row, mut x: f32) -> f32 {
        let t = &m.theme;
        let replay_open = self.replay_open;
        self.icon_button(
            ui,
            m,
            Hit::Replay,
            ICON_REPLAY,
            x + BUTTON / 2.0,
            row.cy,
            ICON_SIZE,
            replay_open,
        );
        if m.replay.is_on() {
            // A small red dot while the buffer is running.
            let pulse = if m.replay.state == ReplayState::Starting && self.animations {
                self.blink()
            } else {
                1.0
            };
            ui.rect(
                Rect::new(x + BUTTON / 2.0 + 3.0, row.cy - 12.0, 9.0, 9.0),
                4.5,
                t.error.with_alpha(pulse),
            );
        }
        if m.mic.state == MicState::Live {
            // The microphone is open: a red microphone at the button's corner.
            ui.icon(
                ICON_MIC,
                x + BUTTON / 2.0 + 10.0,
                row.cy + 10.0,
                11.0,
                t.error,
            );
        }
        x += BUTTON + 4.0;
        self.save_button(ui, m, x + BUTTON / 2.0, row.cy);
        x += BUTTON + 4.0;
        self.record_button(ui, m, x + BUTTON / 2.0, row.cy);
        x + BUTTON
    }

    /// The row of actions at the right: sources, scenes, settings, Studio, full
    /// screen.
    fn pill_actions(&mut self, ui: &mut Ui, m: &Model, row: Row, mut x: f32) {
        let sources_open = self.sources_open;
        self.icon_button(
            ui,
            m,
            Hit::Sources,
            ICON_SOURCE,
            x + BUTTON / 2.0,
            row.cy,
            ICON_SIZE,
            sources_open,
        );
        x += BUTTON + 4.0;
        self.scenes_button(ui, m, x + BUTTON / 2.0, row.cy);
        x += BUTTON + 4.0;
        let settings_open = self.settings_open;
        self.icon_button(
            ui,
            m,
            Hit::Settings,
            ICON_SETTINGS,
            x + BUTTON / 2.0,
            row.cy,
            ICON_SIZE,
            settings_open,
        );
        x += BUTTON + 4.0;
        let studio_open = m.studio_visible();
        let studio_icon = if studio_open {
            ICON_STUDIO_CLOSE
        } else {
            ICON_STUDIO_OPEN
        };
        self.icon_button(
            ui,
            m,
            Hit::Studio,
            studio_icon,
            x + BUTTON / 2.0,
            row.cy,
            ICON_SIZE,
            studio_open,
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
            row.cy,
            ICON_SIZE,
            false,
        );
    }

    /// A quick on-off pulse for something that is starting.
    fn blink(&self) -> f32 {
        0.5 + 0.5 * (self.time * TAU / PULSE_SECS).sin().abs()
    }

    /// The pill's "save clip" button: dimmed while the buffer is off, and
    /// pulsing while a clip is being written.
    pub(super) fn save_button(&mut self, ui: &mut Ui, m: &Model, cx: f32, cy: f32) {
        let t = &m.theme;
        let rect = Rect::new(cx - BUTTON / 2.0, cy - BUTTON / 2.0, BUTTON, BUTTON);
        self.register(Hit::ReplaySave, rect);
        let (_, press) = self.wash(ui, m, Hit::ReplaySave, rect, BUTTON / 2.0, 1.0);
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
        let (_, press) = self.wash(ui, m, Hit::Record, rect, BUTTON / 2.0, 1.0);
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
                    self.blink()
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
