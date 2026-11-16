//! The Recording section: the instant replay (its switch, its length and what
//! it holds), the quality of clips and recordings, the buttons that save a
//! clip and start or stop a recording, and the folder they go to. The same
//! choices, texts and actions as the replay popover and the pill.

use dockview_domain::i18n::{duration_label, size_label, tr, Key};
use dockview_domain::settings::{recording_bytes_per_hour, RecordingQuality};
use dockview_domain::view::{Action, Model, RecState, ReplayState};

use super::widgets::{GAP, LABEL_H, ROW_H};
use super::{StudioPanel, PAD};
use crate::fit::fit_text;
use crate::stats::wrap_text;
use crate::text::Weight;
use crate::{Rect, TextStyle, Ui};

const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 14.0;
const STATUS_LINE: f32 = 18.0;
const HINT_LINE: f32 = 16.0;
/// Height of the clip length stepper, and of the big buttons.
const STEPPER_H: f32 = 40.0;
const BUTTON_H: f32 = 40.0;
/// The size of the round buttons of the stepper.
const STEP_BUTTON: f32 = 32.0;

const SMALL: TextStyle = TextStyle::new(12.5, Weight::Regular);
const HINT: TextStyle = TextStyle::new(12.0, Weight::Regular);
const SWITCH_TITLE: TextStyle = TextStyle::new(14.0, Weight::SemiBold);
const BUTTON_LABEL: TextStyle = TextStyle::new(13.5, Weight::SemiBold);

const ICON_REPLAY: char = '\u{e1f5}';
const ICON_SAVE: char = '\u{e14d}';
const ICON_FOLDER: char = '\u{e247}';
const ICON_MINUS: char = '\u{e11c}';
const ICON_PLUS: char = '\u{e13d}';

/// The two texts that wrap, as lines for a column `width` wide.
struct Texts {
    status: Vec<String>,
    hint: Vec<String>,
}

/// What the replay is doing, in a sentence.
pub(super) fn status_sentence(m: &Model) -> String {
    let (r, lang) = (&m.replay, m.lang);
    match r.state {
        ReplayState::Off => tr(lang, Key::ReplayOff).to_owned(),
        ReplayState::Starting => tr(lang, Key::ReplayStarting).to_owned(),
        ReplayState::Unavailable => tr(lang, Key::ReplayUnavailable).to_owned(),
        ReplayState::Recording => format!(
            "{} · {} · {:.0} MB",
            tr(lang, Key::ReplayRecording),
            duration_label(r.buffered_seconds.floor() as u32),
            r.buffered_mb
        ),
    }
}

/// What the buffer costs in memory and how to save a clip, in a sentence.
pub(super) fn hint_sentence(m: &Model) -> String {
    let (r, lang) = (&m.replay, m.lang);
    let memory = if r.at_limit {
        Key::ReplayMemoryLimit
    } else {
        Key::ReplayMemory
    };
    format!(
        "{} {}",
        tr(lang, memory).replace("{size}", &size_label(r.memory_bytes)),
        tr(lang, Key::ReplayHint).replace("{hotkey}", &r.hotkey)
    )
}

fn texts(ui: &mut Ui, m: &Model, width: f32) -> Texts {
    Texts {
        status: wrap_text(ui, &status_sentence(m), SMALL, width),
        hint: wrap_text(ui, &hint_sentence(m), HINT, width),
    }
}

/// How tall the body is for a panel `width` wide: the texts take what they
/// need once wrapped.
pub fn body_height(ui: &mut Ui, m: &Model, width: f32) -> f32 {
    let t = texts(ui, m, width - PAD * 2.0);
    PAD_TOP
        + ROW_H
        + 4.0
        + t.status.len() as f32 * STATUS_LINE
        + 10.0
        + LABEL_H
        + STEPPER_H
        + 6.0
        + t.hint.len() as f32 * HINT_LINE
        + GAP
        + LABEL_H
        + ROW_H
        + 4.0
        + 16.0
        + GAP
        + BUTTON_H
        + 8.0
        + BUTTON_H
        + GAP
        + 16.0
        + 6.0
        + ROW_H
        + PAD_BOTTOM
}

/// The label of the record button.
pub(super) fn record_label(m: &Model) -> String {
    if m.rec.is_on() {
        format!(
            "{} · {}",
            tr(m.lang, Key::StudioStopRecording),
            duration_label(m.rec.seconds.floor() as u32)
        )
    } else {
        tr(m.lang, Key::StudioRecord).to_owned()
    }
}

impl StudioPanel {
    /// Draws the body of the Recording section with its top-left corner at the
    /// corner of `body`.
    pub(super) fn draw_recording(&mut self, ui: &mut Ui, m: &Model, body: Rect) {
        let t = &m.theme;
        let lang = m.lang;
        let r = &m.replay;
        let (x, w) = (body.x + PAD, body.w - PAD * 2.0);
        let mut y = body.y + PAD_TOP;
        let text = texts(ui, m, w);

        // The switch of the instant replay.
        let on = r.is_on();
        let row = Rect::new(x, y, w, ROW_H);
        let cy = y + ROW_H / 2.0;
        let track = Rect::new(x + w - 42.0 - 6.0, cy - 11.0, 42.0, 22.0);
        self.switch(ui, t, "replay", on, track, row, Action::ToggleReplay);
        ui.icon(
            ICON_REPLAY,
            x + 16.0,
            cy,
            17.0,
            if on { t.error } else { t.text_dim },
        );
        let title_h = ui.measure("Ag", SWITCH_TITLE).1;
        ui.text(
            tr(lang, Key::InstantReplay),
            x + 36.0,
            cy - title_h / 2.0,
            SWITCH_TITLE,
            t.text,
        );
        y += ROW_H + 4.0;

        for line in &text.status {
            ui.text(line, x, y, SMALL, t.text_dim);
            y += STATUS_LINE;
        }
        y += 10.0;

        // How much it keeps: [ - ]  value  [ + ]
        y += self.caption(ui, t, tr(lang, Key::ReplayKeep), x, y);
        self.stepper(ui, m, (x, y, w));
        y += STEPPER_H + 6.0;
        for line in &text.hint {
            ui.text(line, x, y, HINT, t.text_dim);
            y += HINT_LINE;
        }
        y += GAP;

        // The quality of clips and recordings.
        y += self.caption(ui, t, tr(lang, Key::RecordingQuality), x, y);
        let current = RecordingQuality::from_mbps(r.bitrate_mbps);
        let selected = RecordingQuality::ALL
            .iter()
            .position(|q| *q == current)
            .unwrap_or(1);
        y += self.segmented(
            ui,
            t,
            "recording-quality",
            &[tr(lang, Key::QualityNormal), tr(lang, Key::QualityHigh)],
            selected,
            RecordingQuality::ALL
                .iter()
                .map(|q| Action::SetQuality(*q))
                .collect(),
            x,
            y,
            w,
        ) + 4.0;
        let per_hour = tr(lang, Key::QualityHint).replace(
            "{size}",
            &size_label(recording_bytes_per_hour(r.bitrate_mbps)),
        );
        ui.text(&per_hour, x, y, HINT, t.text_dim);
        y += 16.0 + GAP;

        // Save a clip and record.
        let saving = if r.saving {
            Key::ToastSaving
        } else {
            Key::SaveClip
        };
        self.big_button(
            ui,
            m,
            (x, y, w),
            tr(lang, saving),
            ICON_SAVE,
            Action::SaveReplay,
            on,
        );
        y += BUTTON_H + 8.0;
        let recording = m.rec.state != RecState::Idle;
        self.record_button(ui, m, (x, y, w), &record_label(m), recording);
        y += BUTTON_H + GAP;

        // The folder, with a button to open it and one to choose another.
        let path = fit_text(ui, &r.folder, HINT, w);
        ui.text(&path, x, y, HINT, t.text_dim);
        y += 16.0 + 6.0;
        let half = (w - 8.0) / 2.0;
        self.folder_button(
            ui,
            m,
            (x, y, half),
            tr(lang, Key::OpenFolder),
            true,
            Action::OpenReplayFolder,
        );
        self.folder_button(
            ui,
            m,
            (x + half + 8.0, y, half),
            tr(lang, Key::ChangeFolder),
            false,
            Action::ChooseFolder,
        );
    }

    /// `[ - ]  30 s  [ + ]` on a track.
    fn stepper(&mut self, ui: &mut Ui, m: &Model, (x, y, w): (f32, f32, f32)) {
        let t = &m.theme;
        ui.rect(Rect::new(x, y, w, STEPPER_H), 12.0, t.segment_track);
        for (icon, delta, bx) in [
            (ICON_MINUS, -1, x + 4.0),
            (ICON_PLUS, 1, x + w - 4.0 - STEP_BUTTON),
        ] {
            let button = Rect::new(
                bx,
                y + (STEPPER_H - STEP_BUTTON) / 2.0,
                STEP_BUTTON,
                STEP_BUTTON,
            );
            let hit = self.item(button, Action::ReplayLength(delta));
            let hover = self.hover_amount(hit);
            ui.rect(button, 10.0, t.hover.with_alpha(t.hover.a * hover));
            let (cx, cy) = button.center();
            ui.icon(icon, cx, cy, 15.0, t.text);
        }
        let value = duration_label(m.replay.keep_seconds);
        let style = TextStyle::new(15.0, Weight::Medium).tabular();
        let (value_w, value_h) = (ui.measure(&value, style).0, ui.measure("Ag", style).1);
        ui.text(
            &value,
            x + (w - value_w) / 2.0,
            y + (STEPPER_H - value_h) / 2.0,
            style,
            t.text,
        );
    }

    /// A wide button with an icon and a label; with `accent` it is filled.
    #[allow(clippy::too_many_arguments)]
    fn big_button(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        (x, y, w): (f32, f32, f32),
        label: &str,
        icon: char,
        action: Action,
        accent: bool,
    ) {
        let t = &m.theme;
        let rect = Rect::new(x, y, w, BUTTON_H);
        let hit = self.item(rect, action);
        let hover = self.hover_amount(hit);
        let (fill, ink) = if accent {
            let on_accent = match t.appearance {
                dockview_domain::theme::Appearance::Dark => {
                    dockview_domain::theme::Color::hex(0x04141F, 1.0)
                }
                dockview_domain::theme::Appearance::Light => {
                    dockview_domain::theme::Color::hex(0xFFFFFF, 1.0)
                }
            };
            (t.accent, on_accent)
        } else {
            (t.segment_track, t.text_dim)
        };
        ui.rect(rect, 12.0, fill);
        if accent && hover > 0.002 {
            ui.rect(
                rect,
                12.0,
                dockview_domain::theme::Color::hex(0xFFFFFF, 0.14 * hover),
            );
        } else if hover > 0.002 {
            ui.rect(rect, 12.0, t.hover.with_alpha(t.hover.a * hover));
        }
        let label_w = ui.measure(label, BUTTON_LABEL).0 + 26.0;
        let label_h = ui.measure("Ag", BUTTON_LABEL).1;
        let start = x + (w - label_w) / 2.0;
        ui.icon(icon, start + 8.0, y + BUTTON_H / 2.0, 16.0, ink);
        ui.text(
            label,
            start + 26.0,
            y + (BUTTON_H - label_h) / 2.0,
            BUTTON_LABEL,
            ink,
        );
    }

    /// The button that starts or stops a recording: a red circle when idle, a
    /// red fill with a square while recording.
    fn record_button(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        (x, y, w): (f32, f32, f32),
        label: &str,
        recording: bool,
    ) {
        let t = &m.theme;
        let rect = Rect::new(x, y, w, BUTTON_H);
        let hit = self.item(rect, Action::ToggleRecord);
        let hover = self.hover_amount(hit);
        let fill = if recording {
            t.error.with_alpha(0.9)
        } else {
            t.segment_track
        };
        ui.rect(rect, 12.0, fill);
        if hover > 0.002 {
            ui.rect(rect, 12.0, t.hover.with_alpha(t.hover.a * hover));
        }
        let ink = if recording {
            dockview_domain::theme::Color::hex(0xFFFFFF, 1.0)
        } else {
            t.text
        };
        let label_w = ui.measure(label, BUTTON_LABEL).0 + 26.0;
        let label_h = ui.measure("Ag", BUTTON_LABEL).1;
        let start = x + (w - label_w) / 2.0;
        let cy = y + BUTTON_H / 2.0;
        let size = 12.0;
        let (radius, color) = if recording {
            (3.0, ink)
        } else {
            (size / 2.0, t.error)
        };
        ui.rect(
            Rect::new(start + 8.0 - size / 2.0, cy - size / 2.0, size, size),
            radius,
            color,
        );
        ui.text(
            label,
            start + 26.0,
            y + (BUTTON_H - label_h) / 2.0,
            BUTTON_LABEL,
            ink,
        );
    }

    /// One of the two small buttons under the folder's path.
    fn folder_button(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        (x, y, w): (f32, f32, f32),
        label: &str,
        with_icon: bool,
        action: Action,
    ) {
        let t = &m.theme;
        let rect = Rect::new(x, y, w, ROW_H);
        let hit = self.item(rect, action);
        let hover = self.hover_amount(hit);
        ui.rect(rect, 10.0, t.segment_track);
        if hover > 0.002 {
            ui.rect(rect, 10.0, t.hover.with_alpha(t.hover.a * hover));
        }
        let style = TextStyle::new(13.0, Weight::Regular);
        let label_w = ui.measure(label, style).0;
        let icon_w = if with_icon { 22.0 } else { 0.0 };
        let start = x + (w - label_w - icon_w) / 2.0;
        let cy = y + ROW_H / 2.0;
        if with_icon {
            ui.icon(ICON_FOLDER, start + 8.0, cy, 15.0, t.text_dim);
        }
        let line_h = ui.measure("Ag", style).1;
        ui.text(label, start + icon_w, cy - line_h / 2.0, style, t.text);
    }
}
