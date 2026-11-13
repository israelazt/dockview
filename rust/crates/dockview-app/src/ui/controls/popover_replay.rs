//! The instant-replay popover: buffer, clip length, quality, microphone, save
//! and folder. Each part is its own function; `draw_replay_popover` measures,
//! places the panel and runs them top to bottom.

use super::*;

const TITLE_H: f32 = 30.0;
const STATUS_LINE_H: f32 = 18.0;
const HINT_LINE_H: f32 = 16.0;
const STEPPER_H: f32 = 40.0;
const SAVE_H: f32 = 40.0;
const FOLDER_H: f32 = 32.0;

const SMALL: TextStyle = TextStyle::new(12.5, Weight::Regular);
const HINT: TextStyle = TextStyle::new(12.0, Weight::Regular);

/// The column the popover's content goes in, and the height of one text line.
#[derive(Clone, Copy)]
struct Pane {
    x: f32,
    w: f32,
    line_h: f32,
}

/// The texts, wrapped to the popover's width, and the height they give it.
/// Measured before anything is drawn because the panel's size depends on them.
struct ReplayLayout {
    status_lines: Vec<String>,
    hint_lines: Vec<String>,
    height: f32,
}

impl Controls {
    /// The replay popover: switch, clip length, save and folder.
    pub(super) fn draw_replay_popover(&mut self, ui: &mut Ui, m: &Model, pill: Rect) {
        let t = &m.theme;
        let width = REPLAY_POPOVER_WIDTH;
        let layout = replay_layout(ui, m);
        let rect = self.place_replay_popover(ui, pill, width, layout.height);
        self.register(Hit::Popover, rect);
        ui.shadow(rect, 20.0, 26.0, t.shadow);
        ui.panel(rect, 20.0, t.surface, t.border, 1.0);

        let pane = Pane {
            x: rect.x + POPOVER_PAD,
            w: width - POPOVER_PAD * 2.0,
            line_h: ui.measure("Ag", BODY).1,
        };
        let mut y = rect.y + POPOVER_PAD;
        y = self.replay_title(ui, m, pane, y);
        y = self.replay_status(ui, m, pane, y, &layout.status_lines);
        y = self.replay_length(ui, m, pane, y);
        y = self.replay_hint(ui, m, pane, y, &layout.hint_lines);
        y = self.replay_quality(ui, m, pane, y);
        y = self.replay_mic(ui, m, pane, y);
        y = self.replay_save(ui, m, pane, y);
        self.replay_folder(ui, m, pane, y);
    }

    /// Where the panel goes: centred over the replay button, kept on screen,
    /// and above the pill.
    fn place_replay_popover(&self, ui: &Ui, pill: Rect, width: f32, height: f32) -> Rect {
        let (screen_w, _) = ui.logical_size();
        let anchor = self
            .next_hits
            .iter()
            .find(|(h, _)| *h == Hit::Replay)
            .map_or(pill.x + pill.w / 2.0, |(_, rect)| rect.center().0);
        let x = (anchor - width / 2.0)
            .min(screen_w - width - POPOVER_GAP)
            .max(POPOVER_GAP)
            .round();
        let top = (pill.y - POPOVER_GAP - height).max(POPOVER_GAP).round();
        Rect::new(x, top, width, height)
    }

    /// Title row: the whole row toggles the buffer.
    fn replay_title(&mut self, ui: &mut Ui, m: &Model, pane: Pane, y: f32) -> f32 {
        let t = &m.theme;
        let on = m.replay.is_on();
        self.toggle_row(
            ui,
            m,
            Rect::new(pane.x, y, pane.w, TITLE_H),
            ToggleRow {
                icon: ICON_REPLAY,
                icon_color: if on { t.error } else { t.text_dim },
                title: Key::InstantReplay,
                on,
                hit: Hit::ReplayToggle,
                knob: Hit::ReplayKnob,
            },
        );
        y + TITLE_H + 4.0
    }

    fn replay_status(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        pane: Pane,
        mut y: f32,
        lines: &[String],
    ) -> f32 {
        for line in lines {
            ui.text(line, pane.x, y, SMALL, m.theme.text_dim);
            y += STATUS_LINE_H;
        }
        y + 14.0
    }

    /// Clip length: [ − ]  value  [ + ]
    fn replay_length(&mut self, ui: &mut Ui, m: &Model, pane: Pane, mut y: f32) -> f32 {
        let t = &m.theme;
        ui.text(tr(m.lang, Key::ReplayKeep), pane.x, y, SECTION, t.text_dim);
        y += LABEL_HEIGHT;
        let track = Rect::new(pane.x, y, pane.w, STEPPER_H);
        ui.rect(track, 12.0, t.segment_track);
        let step_hits = [
            (Hit::ReplayShorter, ICON_MINUS, track.x + 4.0),
            (
                Hit::ReplayLonger,
                ICON_PLUS,
                track.x + track.w - 4.0 - BUTTON,
            ),
        ];
        for (hit, icon, bx) in step_hits {
            let button = Rect::new(bx, y + (STEPPER_H - BUTTON) / 2.0, BUTTON, BUTTON);
            self.register(hit, button);
            let (_, press) = self.wash(ui, m, hit, button, 10.0, 1.0);
            let (cx, cy) = button.center();
            ui.icon(icon, cx, cy, 15.0 * (1.0 - 0.1 * press), t.text);
        }
        let value = dockview_domain::i18n::duration_label(m.replay.keep_seconds);
        let value_style = TextStyle::new(15.0, Weight::Medium).tabular();
        let value_w = ui.measure(&value, value_style).0;
        let value_h = ui.measure("Ag", value_style).1;
        ui.text(
            &value,
            track.x + (track.w - value_w) / 2.0,
            y + (STEPPER_H - value_h) / 2.0,
            value_style,
            t.text,
        );
        y + STEPPER_H + 8.0
    }

    fn replay_hint(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        pane: Pane,
        mut y: f32,
        lines: &[String],
    ) -> f32 {
        for line in lines {
            ui.text(line, pane.x, y, HINT, m.theme.text_dim);
            y += HINT_LINE_H;
        }
        y + 14.0
    }

    /// Recording quality: one encoder serves the buffer and the recordings.
    fn replay_quality(&mut self, ui: &mut Ui, m: &Model, pane: Pane, y: f32) -> f32 {
        let t = &m.theme;
        let r = &m.replay;
        let qualities = [tr(m.lang, Key::QualityNormal), tr(m.lang, Key::QualityHigh)];
        let selected = RecordingQuality::ALL
            .iter()
            .position(|q| *q == RecordingQuality::from_mbps(r.bitrate_mbps))
            .unwrap_or(1);
        let target = selected as f32;
        let position = self.approach(self.quality_slide.unwrap_or(target), target, SLIDE_RATE);
        self.quality_slide = Some(position);
        let mut y = self.segmented_section(
            ui,
            m,
            Key::RecordingQuality,
            &qualities,
            selected,
            position,
            Hit::Quality,
            pane.x,
            y,
            pane.w,
        );
        y += 4.0;
        let per_hour = tr(m.lang, Key::QualityHint).replace(
            "{size}",
            &dockview_domain::i18n::size_label(recording_bytes_per_hour(r.bitrate_mbps)),
        );
        ui.text(&per_hour, pane.x, y, HINT, t.text_dim);
        y + 16.0 + 14.0
    }

    /// Microphone: switch, and a level meter while it is open.
    fn replay_mic(&mut self, ui: &mut Ui, m: &Model, pane: Pane, mut y: f32) -> f32 {
        let t = &m.theme;
        let (icon, icon_color) = match m.mic.state {
            MicState::Live | MicState::Starting => (ICON_MIC, t.error),
            MicState::Unavailable | MicState::Off => (ICON_MIC_OFF, t.text_dim),
        };
        self.toggle_row(
            ui,
            m,
            Rect::new(pane.x, y, pane.w, MIC_ROW),
            ToggleRow {
                icon,
                icon_color,
                title: Key::Microphone,
                on: m.mic.is_on(),
                hit: Hit::MicToggle,
                knob: Hit::MicKnob,
            },
        );
        y += MIC_ROW + 4.0;
        match m.mic.state {
            MicState::Unavailable => {
                ui.text(tr(m.lang, Key::MicUnavailable), pane.x, y, HINT, t.error);
            }
            MicState::Off => {
                ui.text(tr(m.lang, Key::MicHint), pane.x, y, HINT, t.text_dim);
            }
            MicState::Starting | MicState::Live => {
                let bar = Rect::new(pane.x, y + (MIC_METER - 4.0) / 2.0, pane.w, 4.0);
                ui.rect(bar, 2.0, t.track);
                // Loudness feels linear on a square-root scale.
                let filled = m.mic.level.clamp(0.0, 1.0).sqrt() * bar.w;
                if filled >= 1.0 {
                    ui.rect(Rect::new(bar.x, bar.y, filled, bar.h), 2.0, t.accent);
                }
            }
        }
        y + MIC_METER + 14.0
    }

    /// The save button: accent while the buffer runs, flat when it is off.
    fn replay_save(&mut self, ui: &mut Ui, m: &Model, pane: Pane, y: f32) -> f32 {
        let t = &m.theme;
        let r = &m.replay;
        let button = Rect::new(pane.x, y, pane.w, SAVE_H);
        self.register(Hit::ReplayPopSave, button);
        let (hover, press) = self.amounts(Hit::ReplayPopSave);
        let enabled = r.is_on();
        let (fill, label_color) = if enabled {
            let on_accent = match t.appearance {
                dockview_domain::theme::Appearance::Dark => Color::hex(0x04141F, 1.0),
                dockview_domain::theme::Appearance::Light => Color::hex(0xFFFFFF, 1.0),
            };
            (t.accent, on_accent)
        } else {
            (t.segment_track, t.text_dim)
        };
        ui.rect(button, 12.0, fill);
        let sheen = Color::hex(0xFFFFFF, 0.0).with_alpha(0.14 * hover + 0.14 * press);
        if enabled && sheen.a > 0.002 {
            ui.rect(button, 12.0, sheen);
        }
        let label = if r.saving {
            tr(m.lang, Key::ToastSaving)
        } else {
            tr(m.lang, Key::SaveClip)
        };
        let label_style = TextStyle::new(13.5, Weight::SemiBold);
        let label_w = ui.measure(label, label_style).0 + 26.0;
        let label_h = ui.measure("Ag", label_style).1;
        let start = button.x + (button.w - label_w) / 2.0;
        ui.icon(ICON_SAVE, start + 8.0, y + SAVE_H / 2.0, 16.0, label_color);
        ui.text(
            label,
            start + 26.0,
            y + (SAVE_H - label_h) / 2.0,
            label_style,
            label_color,
        );
        y + SAVE_H + 8.0
    }

    /// Folder: where files go, with buttons to open it or choose another.
    fn replay_folder(&mut self, ui: &mut Ui, m: &Model, pane: Pane, mut y: f32) {
        let t = &m.theme;
        let path = fit_text(ui, &m.replay.folder, HINT, pane.w);
        ui.text(&path, pane.x, y, HINT, t.text_dim);
        y += 16.0 + 6.0;
        let half = (pane.w - 8.0) / 2.0;
        let cy = y + FOLDER_H / 2.0;
        let buttons = [
            (Hit::ReplayFolder, Key::OpenFolder, pane.x, true),
            (
                Hit::ReplayChangeFolder,
                Key::ChangeFolder,
                pane.x + half + 8.0,
                false,
            ),
        ];
        for (hit, key, bx, with_icon) in buttons {
            let rect = Rect::new(bx, y, half, FOLDER_H);
            self.register(hit, rect);
            // The resting fill goes under the highlight.
            let (hover, press) = self.amounts(hit);
            ui.rect(rect, 10.0, t.segment_track);
            let alpha = t.hover.a * (hover + 0.9 * press);
            if alpha > 0.002 {
                ui.rect(rect, 10.0, t.hover.with_alpha(alpha.min(1.0)));
            }
            let label = tr(m.lang, key);
            let label_w = ui.measure(label, BODY).0;
            let icon_w = if with_icon { 22.0 } else { 0.0 };
            let start = rect.x + (rect.w - label_w - icon_w) / 2.0;
            if with_icon {
                ui.icon(ICON_FOLDER, start + 8.0, cy, 15.0, t.text_dim);
            }
            ui.text(label, start + icon_w, cy - pane.line_h / 2.0, BODY, t.text);
        }
    }
}

/// Wraps the texts to the popover's width and adds up its height.
fn replay_layout(ui: &mut Ui, m: &Model) -> ReplayLayout {
    let r = &m.replay;
    let lang = m.lang;
    let inner_w = REPLAY_POPOVER_WIDTH - POPOVER_PAD * 2.0;

    let status = match r.state {
        ReplayState::Off => tr(lang, Key::ReplayOff).to_owned(),
        ReplayState::Starting => tr(lang, Key::ReplayStarting).to_owned(),
        ReplayState::Unavailable => tr(lang, Key::ReplayUnavailable).to_owned(),
        ReplayState::Recording => format!(
            "{} · {} · {:.0} MB",
            tr(lang, Key::ReplayRecording),
            dockview_domain::i18n::duration_label(r.buffered_seconds.floor() as u32),
            r.buffered_mb
        ),
    };
    let status_lines = wrap_text(ui, &status, SMALL, inner_w);
    let memory_key = if r.at_limit {
        Key::ReplayMemoryLimit
    } else {
        Key::ReplayMemory
    };
    let hint = format!(
        "{} {}",
        tr(lang, memory_key).replace("{size}", &dockview_domain::i18n::size_label(r.memory_bytes)),
        tr(lang, Key::ReplayHint).replace("{hotkey}", &r.hotkey)
    );
    let hint_lines = wrap_text(ui, &hint, HINT, inner_w);

    let status_h = status_lines.len() as f32 * STATUS_LINE_H;
    let hint_h = hint_lines.len() as f32 * HINT_LINE_H;
    let height = POPOVER_PAD * 2.0
        + TITLE_H
        + 4.0
        + status_h
        + 14.0
        + LABEL_HEIGHT
        + STEPPER_H
        + 8.0
        + hint_h
        + 14.0
        + LABEL_HEIGHT
        + ROW_HEIGHT
        + 4.0
        + 16.0
        + 14.0
        + MIC_ROW
        + 4.0
        + MIC_METER
        + 14.0
        + SAVE_H
        + 8.0
        + 16.0
        + 6.0
        + FOLDER_H;
    ReplayLayout {
        status_lines,
        hint_lines,
        height,
    }
}
