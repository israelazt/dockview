//! The Audio section: the volume, the sync, the audio input and the
//! microphone with its level, the same choices (and actions) as the pill and
//! the popovers.

use dockview_domain::i18n::{tr, Key};
use dockview_domain::settings::MAX_AUDIO_OFFSET_MS;
use dockview_domain::view::{Action, MicState, Model};

use super::widgets::{list_height, GAP, LABEL_H, ROW_H};
use super::{StudioPanel, NOTE, PAD};
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

/// Audio sync change per click, in milliseconds (the same as the pill).
const SYNC_STEP_MS: i32 = 10;
/// Most inputs listed.
const MAX_INPUTS: usize = 5;
const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 14.0;
/// Height of the microphone's level meter and the space around it.
const METER_H: f32 = 22.0;

const VALUE: TextStyle = TextStyle::new(13.0, Weight::Regular).tabular();
const SWITCH_TITLE: TextStyle = TextStyle::new(14.0, Weight::SemiBold);

const ICON_MIC: char = '\u{e118}';
const ICON_MIC_OFF: char = '\u{e119}';
const ICON_MINUS: char = '\u{e11c}';
const ICON_PLUS: char = '\u{e13d}';

/// How tall the body is.
pub fn body_height(m: &Model) -> f32 {
    PAD_TOP
        + (LABEL_H + ROW_H) // volume
        + GAP
        + (LABEL_H + ROW_H) // sync
        + GAP
        + (LABEL_H + list_height(m.audio_devices.len(), MAX_INPUTS)) // audio input
        + GAP
        + (ROW_H + METER_H + list_height(m.mic_devices.len(), MAX_INPUTS)) // microphone
        + PAD_BOTTOM
}

/// The volume asked for by dragging the slider to `value` (0 to 1).
fn volume(value: f32) -> Action {
    Action::SetVolume(value)
}

impl StudioPanel {
    /// Draws the body of the Audio section with its top-left corner at the
    /// corner of `body`.
    pub(super) fn draw_audio(&mut self, ui: &mut Ui, m: &Model, body: Rect) {
        let t = &m.theme;
        let lang = m.lang;
        let (x, w) = (body.x + PAD, body.w - PAD * 2.0);
        let mut y = body.y + PAD_TOP;

        // Volume: the slider and the percent beside it.
        y += self.caption(ui, t, tr(lang, Key::StudioVolume), x, y);
        let percent = format!("{:.0}%", m.volume * 100.0);
        let percent_w = ui.measure("100%", VALUE).0;
        self.slider(ui, t, m.volume, volume, x + 4.0, y, w - percent_w - 18.0);
        let line_h = ui.measure("Ag", VALUE).1;
        ui.text(
            &percent,
            x + w - percent_w,
            y + (ROW_H - line_h) / 2.0,
            VALUE,
            t.text_dim,
        );
        y += ROW_H + GAP;

        // Sync: earlier, the value and later.
        y += self.caption(ui, t, tr(lang, Key::Sync), x, y);
        self.sync_row(ui, m, x, y, w);
        y += ROW_H + GAP;

        // The audio input of the capture card.
        y += self.caption(ui, t, tr(lang, Key::Audio), x, y);
        y += self.list(
            ui,
            t,
            lang,
            &m.audio_devices,
            m.audio_selected,
            MAX_INPUTS,
            &Action::SelectAudio,
            x,
            y,
            w,
        ) + GAP;

        // The microphone: its switch, its level and its devices.
        y += self.mic_switch(ui, m, x, y, w);
        y += self.mic_meter(ui, m, x, y, w);
        self.list(
            ui,
            t,
            lang,
            &m.mic_devices,
            m.mic_selected,
            MAX_INPUTS,
            &Action::SelectMic,
            x,
            y,
            w,
        );
    }

    /// `[ - ]  +0 ms  [ + ]`, centred.
    fn sync_row(&mut self, ui: &mut Ui, m: &Model, x: f32, y: f32, w: f32) {
        let t = &m.theme;
        let clamp = |ms: i32| ms.clamp(-MAX_AUDIO_OFFSET_MS, MAX_AUDIO_OFFSET_MS);
        let value = format!("{:+} ms", m.sync_ms).replace('-', "\u{2212}");
        let value_w = ui.measure("\u{2212}300 ms", VALUE).0;
        let total = ROW_H * 2.0 + value_w;
        let left = x + (w - total) / 2.0;
        let line_h = ui.measure("Ag", VALUE).1;
        for (i, (icon, action)) in [
            (ICON_MINUS, Action::SetSync(clamp(m.sync_ms - SYNC_STEP_MS))),
            (ICON_PLUS, Action::SetSync(clamp(m.sync_ms + SYNC_STEP_MS))),
        ]
        .into_iter()
        .enumerate()
        {
            let bx = if i == 0 { left } else { left + ROW_H + value_w };
            let button = Rect::new(bx, y, ROW_H, ROW_H);
            let hit = self.item(button, action);
            let hover = self.hover_amount(hit);
            ui.rect(button, ROW_H / 2.0, t.hover.with_alpha(t.hover.a * hover));
            let (cx, cy) = button.center();
            ui.icon(icon, cx, cy, 14.0, t.text);
        }
        let text_w = ui.measure(&value, VALUE).0;
        ui.text(
            &value,
            left + ROW_H + (value_w - text_w) / 2.0,
            y + (ROW_H - line_h) / 2.0,
            VALUE,
            t.text,
        );
    }

    /// The row with the microphone's icon, its name and its switch.
    fn mic_switch(&mut self, ui: &mut Ui, m: &Model, x: f32, y: f32, w: f32) -> f32 {
        let t = &m.theme;
        let row = Rect::new(x, y, w, ROW_H);
        let on = m.mic.is_on();
        let cy = y + ROW_H / 2.0;
        let track = Rect::new(x + w - 42.0 - 6.0, cy - 11.0, 42.0, 22.0);
        self.switch(ui, t, "mic", on, track, row, Action::ToggleMic);
        let icon = if on { ICON_MIC } else { ICON_MIC_OFF };
        let color = if m.mic.state == MicState::Live {
            t.error
        } else {
            t.text_dim
        };
        ui.icon(icon, x + 16.0, cy, 17.0, color);
        let line_h = ui.measure("Ag", SWITCH_TITLE).1;
        ui.text(
            tr(m.lang, Key::Microphone),
            x + 36.0,
            cy - line_h / 2.0,
            SWITCH_TITLE,
            t.text,
        );
        ROW_H
    }

    /// The level bar while the microphone is open; a note when it is not.
    fn mic_meter(&mut self, ui: &mut Ui, m: &Model, x: f32, y: f32, w: f32) -> f32 {
        let t = &m.theme;
        let bar = Rect::new(x + 10.0, y + (METER_H - 4.0) / 2.0, w - 20.0, 4.0);
        match m.mic.state {
            MicState::Live => {
                ui.rect(bar, 2.0, t.track);
                // The square root makes quiet sounds visible.
                let filled = m.mic.level.clamp(0.0, 1.0).sqrt() * bar.w;
                ui.rect(Rect::new(bar.x, bar.y, filled, bar.h), 2.0, t.accent);
            }
            MicState::Unavailable => {
                let line_h = ui.measure("Ag", NOTE).1;
                ui.text(
                    tr(m.lang, Key::MicUnavailable),
                    bar.x,
                    y + (METER_H - line_h) / 2.0,
                    NOTE,
                    t.error,
                );
            }
            MicState::Off | MicState::Starting => {
                ui.rect(bar, 2.0, t.track.with_alpha(t.track.a * 0.5));
            }
        }
        METER_H
    }
}
