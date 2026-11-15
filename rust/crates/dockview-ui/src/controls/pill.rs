//! The floating bar: save a clip, record, take a screenshot, volume and sync,
//! the microphone, the readouts and full screen.

use super::buttons::{chip_width, record_label};
use super::*;

/// The widths the bar's parts need. They use the widest plausible value so the
/// bar does not resize while numbers change.
pub(super) struct BarMetrics {
    pub(super) save_w: f32,
    pub(super) record_w: f32,
    pub(super) capture_w: f32,
    percent_w: f32,
    sync_value_w: f32,
    /// The whole bar.
    width: f32,
    /// Whether the sync block is drawn. It is left out first when the bar does
    /// not fit the room it has.
    show_sync: bool,
    /// Whether the hotkeys are written in the buttons. They go second.
    pub(super) show_chips: bool,
}

/// Where a group of controls sits vertically.
#[derive(Clone, Copy)]
pub(super) struct Row {
    /// Centre line of the bar.
    pub(super) cy: f32,
    /// Top of a line of text centred on it.
    pub(super) text_y: f32,
}

fn bar_metrics(ui: &mut Ui, m: &Model, room: f32) -> BarMetrics {
    let value = BODY.tabular();
    let hotkey_w = |ui: &mut Ui, text: &str| {
        if text.is_empty() {
            0.0
        } else {
            GAP + chip_width(ui, text)
        }
    };
    let save_label = ui.measure(tr(m.lang, Key::BarSave), BUTTON_TEXT).0;
    let record_text = ui
        .measure(&record_label(m), BUTTON_TEXT)
        .0
        .max(ui.measure(tr(m.lang, Key::BarRecord), BUTTON_TEXT).0)
        .max(ui.measure("0:00:00", BUTTON_TEXT.tabular()).0);
    let capture_label = ui.measure(tr(m.lang, Key::BarCapture), BUTTON_TEXT).0;
    let save_chip = hotkey_w(ui, &m.replay.hotkey);
    let record_chip = hotkey_w(ui, &m.replay.record_hotkey);
    let percent_w = ui.measure("100%", value).0;
    let sync_value_w = ui.measure("−300 ms", value).0;

    let wide = |content: f32| 14.0 + content + 14.0;
    let save = |chips: bool| {
        wide(ICON_SIZE - 2.0 + 8.0 + save_label + if chips { save_chip } else { 0.0 })
    };
    let record =
        |chips: bool| wide(12.0 + 8.0 + record_text + if chips { record_chip } else { 0.0 });
    let capture_w = wide(ICON_SIZE - 2.0 + 8.0 + capture_label);
    let volume = ICON_SIZE + 8.0 + SLIDER_WIDTH + 8.0 + percent_w;
    let sync = BUTTON + sync_value_w + BUTTON;
    let divider = DIVIDER_PAD * 2.0 + 1.0;
    let round = ITEM_H * 3.0 + GAP * 2.0;
    let width_of = |chips: bool, with_sync: bool| {
        PAD_X * 2.0
            + save(chips)
            + GAP
            + record(chips)
            + GAP
            + capture_w
            + divider
            + volume
            + if with_sync { DIVIDER_PAD + sync } else { 0.0 }
            + divider
            + round
    };
    let (show_chips, show_sync) = if width_of(true, true) <= room {
        (true, true)
    } else if width_of(true, false) <= room {
        (true, false)
    } else {
        (false, false)
    };
    BarMetrics {
        save_w: save(show_chips),
        record_w: record(show_chips),
        capture_w,
        percent_w,
        sync_value_w,
        width: width_of(show_chips, show_sync),
        show_sync,
        show_chips,
    }
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

fn divider_at(ui: &mut Ui, theme: &Theme, x: f32, cy: f32) -> f32 {
    ui.rect(
        Rect::new(x + DIVIDER_PAD, cy - 10.0, 1.0, 20.0),
        0.5,
        theme.track,
    );
    x + DIVIDER_PAD * 2.0 + 1.0
}

impl Controls {
    pub(super) fn draw_pill(&mut self, ui: &mut Ui, m: &Model) -> Rect {
        let t = &m.theme;
        let (screen_w, screen_h) = ui.logical_size();
        let metrics = bar_metrics(ui, m, screen_w - 16.0);

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
        x = self.save_button(ui, m, &metrics, row, x) + GAP;
        x = self.record_button(ui, m, &metrics, row, x) + GAP;
        x = self.capture_button(ui, m, &metrics, row, x);
        x = divider_at(ui, t, x, cy);
        x = self.pill_volume(ui, m, &metrics, row, x);
        if metrics.show_sync {
            x += DIVIDER_PAD;
            x = self.pill_sync(ui, m, &metrics, row, x);
        }
        x = divider_at(ui, t, x, cy);
        self.pill_toggles(ui, m, row, x);
        pill
    }

    fn pill_volume(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        met: &BarMetrics,
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
        x += ICON_SIZE + 8.0;
        self.draw_slider(ui, m, Rect::new(x, row.cy - 6.0, SLIDER_WIDTH, 12.0));
        x += SLIDER_WIDTH + 8.0;
        let percent = format!("{:.0}%", m.volume * 100.0);
        ui.text(&percent, x, row.text_y, BODY.tabular(), t.text_dim);
        x + met.percent_w
    }

    fn pill_sync(&mut self, ui: &mut Ui, m: &Model, met: &BarMetrics, row: Row, mut x: f32) -> f32 {
        let t = &m.theme;
        let value = BODY.tabular();
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

    /// The round buttons at the right: microphone, readouts, full screen.
    fn pill_toggles(&mut self, ui: &mut Ui, m: &Model, row: Row, mut x: f32) {
        let t = &m.theme;
        let round = |this: &mut Self, ui: &mut Ui, hit: Hit, x: f32, fill: Option<Color>| {
            let rect = Rect::new(x, row.cy - ITEM_H / 2.0, ITEM_H, ITEM_H);
            this.register(hit, rect);
            if let Some(fill) = fill {
                ui.rect(rect, ITEM_H / 2.0, fill);
            }
            this.wash(ui, m, hit, rect, ITEM_H / 2.0, 1.0);
            rect.center()
        };
        // The microphone is red while it is open.
        let live = m.mic.state == MicState::Live;
        let fill = live.then(|| t.error.with_alpha(0.18));
        let (cx, cy) = round(self, ui, Hit::Mic, x, fill);
        let (icon, color) = if live {
            (ICON_MIC, t.error)
        } else {
            (ICON_MIC_OFF, t.text_dim)
        };
        ui.icon(icon, cx, cy, ICON_SIZE, color);
        x += ITEM_H + GAP;

        let (cx, cy) = round(self, ui, Hit::Hud, x, None);
        let (icon, color) = if m.hud {
            (ICON_EYE, t.text)
        } else {
            (ICON_EYE_OFF, t.text_dim)
        };
        ui.icon(icon, cx, cy, ICON_SIZE, color);
        x += ITEM_H + GAP;

        let (cx, cy) = round(self, ui, Hit::Fullscreen, x, Some(t.segment_active));
        let icon = if m.fullscreen {
            ICON_EXIT_FULLSCREEN
        } else {
            ICON_FULLSCREEN
        };
        ui.icon(icon, cx, cy, ICON_SIZE, t.text);
    }

    /// A quick on-off pulse for something that is starting.
    pub(super) fn blink(&self) -> f32 {
        0.5 + 0.5 * (self.time * TAU / PULSE_SECS).sin().abs()
    }
}
