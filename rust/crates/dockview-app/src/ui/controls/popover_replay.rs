//! The instant-replay popover: buffer, microphone, quality and folder.

use super::*;

impl Controls {
    /// The replay popover: switch, clip length, save and folder.
    pub(super) fn draw_replay_popover(&mut self, ui: &mut Ui, m: &Model, pill: Rect) {
        let t = &m.theme;
        let r = &m.replay;
        let lang = m.lang;
        let width = REPLAY_POPOVER_WIDTH;
        let inner_w = width - POPOVER_PAD * 2.0;
        let small = TextStyle::new(12.5, Weight::Regular);
        let hint_style = TextStyle::new(12.0, Weight::Regular);

        let status = match r.state {
            ReplayState::Off => tr(lang, Key::ReplayOff).to_owned(),
            ReplayState::Starting => tr(lang, Key::ReplayStarting).to_owned(),
            ReplayState::Unavailable => tr(lang, Key::ReplayUnavailable).to_owned(),
            ReplayState::Recording => format!(
                "{} · {} · {:.0} MB",
                tr(lang, Key::ReplayRecording),
                crate::i18n::duration_label(r.buffered_seconds.floor() as u32),
                r.buffered_mb
            ),
        };
        let status_lines = wrap_text(ui, &status, small, inner_w);
        let memory_key = if r.at_limit {
            Key::ReplayMemoryLimit
        } else {
            Key::ReplayMemory
        };
        let hint = format!(
            "{} {}",
            tr(lang, memory_key).replace("{size}", &crate::i18n::size_label(r.memory_bytes)),
            tr(lang, Key::ReplayHint).replace("{hotkey}", &r.hotkey)
        );
        let hint_lines = wrap_text(ui, &hint, hint_style, inner_w);

        let title_h = 30.0;
        let status_h = status_lines.len() as f32 * 18.0;
        let hint_h = hint_lines.len() as f32 * 16.0;
        let stepper_h = 40.0;
        let button_h = 40.0;
        let folder_h = 32.0;
        let height = POPOVER_PAD * 2.0
            + title_h
            + 4.0
            + status_h
            + 14.0
            + LABEL_HEIGHT
            + stepper_h
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
            + button_h
            + 8.0
            + 16.0
            + 6.0
            + folder_h;

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
        let rect = Rect::new(x, top, width, height);
        self.register(Hit::Popover, rect);
        ui.shadow(rect, 20.0, 26.0, t.shadow);
        ui.panel(rect, 20.0, t.surface, t.border, 1.0);

        let inner_x = x + POPOVER_PAD;
        let mut y = top + POPOVER_PAD;
        let line_h = ui.measure("Ag", BODY).1;

        // Title row: the whole row toggles the buffer.
        let row = Rect::new(inner_x, y, inner_w, title_h);
        self.register(Hit::ReplayToggle, row);
        let (hover, press) = self.amounts(Hit::ReplayToggle);
        let shade = t.hover.a * (hover * 0.6 + 0.9 * press);
        if shade > 0.002 {
            ui.rect(
                Rect::new(row.x - 6.0, row.y - 2.0, row.w + 12.0, row.h + 4.0),
                10.0,
                t.hover.with_alpha(shade.min(1.0)),
            );
        }
        let cy = y + title_h / 2.0;
        let icon_color = if r.is_on() { t.error } else { t.text_dim };
        ui.icon(ICON_REPLAY, inner_x + 10.0, cy, 17.0, icon_color);
        ui.text(
            tr(lang, Key::InstantReplay),
            inner_x + 30.0,
            cy - line_h / 2.0,
            TextStyle::new(14.0, Weight::SemiBold),
            t.text,
        );
        self.switch(
            ui,
            m,
            Rect::new(inner_x + inner_w - 42.0, cy - 11.0, 42.0, 22.0),
            r.is_on(),
            Hit::ReplayKnob,
        );
        y += title_h + 4.0;

        for line in &status_lines {
            ui.text(line, inner_x, y, small, t.text_dim);
            y += 18.0;
        }
        y += 14.0;

        // Clip length: [ − ]  value  [ + ]
        ui.text(tr(lang, Key::ReplayKeep), inner_x, y, SECTION, t.text_dim);
        y += LABEL_HEIGHT;
        let track = Rect::new(inner_x, y, inner_w, stepper_h);
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
            let button = Rect::new(bx, y + (stepper_h - BUTTON) / 2.0, BUTTON, BUTTON);
            self.register(hit, button);
            let (hover, press) = self.amounts(hit);
            let alpha = t.hover.a * (hover + 0.9 * press);
            if alpha > 0.002 {
                ui.rect(button, 10.0, t.hover.with_alpha(alpha.min(1.0)));
            }
            let (cx, cy) = button.center();
            ui.icon(icon, cx, cy, 15.0 * (1.0 - 0.1 * press), t.text);
        }
        let value = crate::i18n::duration_label(r.keep_seconds);
        let value_style = TextStyle::new(15.0, Weight::Medium).tabular();
        let value_w = ui.measure(&value, value_style).0;
        let value_h = ui.measure("Ag", value_style).1;
        ui.text(
            &value,
            track.x + (track.w - value_w) / 2.0,
            y + (stepper_h - value_h) / 2.0,
            value_style,
            t.text,
        );
        y += stepper_h + 8.0;

        for line in &hint_lines {
            ui.text(line, inner_x, y, hint_style, t.text_dim);
            y += 16.0;
        }
        y += 14.0;

        // Recording quality: one encoder serves the buffer and the recordings.
        let qualities = [tr(lang, Key::QualityNormal), tr(lang, Key::QualityHigh)];
        let quality_selected = RecordingQuality::ALL
            .iter()
            .position(|q| *q == RecordingQuality::from_mbps(r.bitrate_mbps))
            .unwrap_or(1);
        let quality_target = quality_selected as f32;
        let quality_pos = self.approach(
            self.quality_slide.unwrap_or(quality_target),
            quality_target,
            SLIDE_RATE,
        );
        self.quality_slide = Some(quality_pos);
        y = self.segmented_section(
            ui,
            m,
            Key::RecordingQuality,
            &qualities,
            quality_selected,
            quality_pos,
            Hit::Quality,
            inner_x,
            y,
            inner_w,
        );
        y += 4.0;
        let per_hour = tr(lang, Key::QualityHint).replace(
            "{size}",
            &crate::i18n::size_label(recording_bytes_per_hour(r.bitrate_mbps)),
        );
        ui.text(&per_hour, inner_x, y, hint_style, t.text_dim);
        y += 16.0 + 14.0;

        // Microphone: switch, and a level meter while it is open.
        let mic_row = Rect::new(inner_x, y, inner_w, MIC_ROW);
        self.register(Hit::MicToggle, mic_row);
        let (hover, press) = self.amounts(Hit::MicToggle);
        let shade = t.hover.a * (hover * 0.6 + 0.9 * press);
        if shade > 0.002 {
            ui.rect(
                Rect::new(
                    mic_row.x - 6.0,
                    mic_row.y - 2.0,
                    mic_row.w + 12.0,
                    mic_row.h + 4.0,
                ),
                10.0,
                t.hover.with_alpha(shade.min(1.0)),
            );
        }
        let mic_cy = y + MIC_ROW / 2.0;
        let (mic_icon, mic_color) = match m.mic.state {
            MicState::Live | MicState::Starting => (ICON_MIC, t.error),
            MicState::Unavailable => (ICON_MIC_OFF, t.text_dim),
            MicState::Off => (ICON_MIC_OFF, t.text_dim),
        };
        ui.icon(mic_icon, inner_x + 10.0, mic_cy, 17.0, mic_color);
        ui.text(
            tr(lang, Key::Microphone),
            inner_x + 30.0,
            mic_cy - line_h / 2.0,
            TextStyle::new(14.0, Weight::SemiBold),
            t.text,
        );
        self.switch(
            ui,
            m,
            Rect::new(inner_x + inner_w - 42.0, mic_cy - 11.0, 42.0, 22.0),
            m.mic.is_on(),
            Hit::MicKnob,
        );
        y += MIC_ROW + 4.0;
        match m.mic.state {
            MicState::Unavailable => {
                ui.text(
                    tr(lang, Key::MicUnavailable),
                    inner_x,
                    y,
                    hint_style,
                    t.error,
                );
            }
            MicState::Off => {
                ui.text(tr(lang, Key::MicHint), inner_x, y, hint_style, t.text_dim);
            }
            MicState::Starting | MicState::Live => {
                let bar = Rect::new(inner_x, y + (MIC_METER - 4.0) / 2.0, inner_w, 4.0);
                ui.rect(bar, 2.0, t.track);
                // Loudness feels linear on a square-root scale.
                let filled = m.mic.level.clamp(0.0, 1.0).sqrt() * bar.w;
                if filled >= 1.0 {
                    ui.rect(Rect::new(bar.x, bar.y, filled, bar.h), 2.0, t.accent);
                }
            }
        }
        y += MIC_METER + 14.0;

        // Save button.
        let button = Rect::new(inner_x, y, inner_w, button_h);
        self.register(Hit::ReplayPopSave, button);
        let (hover, press) = self.amounts(Hit::ReplayPopSave);
        let enabled = r.is_on();
        let (fill, label_color) = if enabled {
            let on_accent = match t.appearance {
                crate::theme::Appearance::Dark => Color::hex(0x04141F, 1.0),
                crate::theme::Appearance::Light => Color::hex(0xFFFFFF, 1.0),
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
            tr(lang, Key::ToastSaving)
        } else {
            tr(lang, Key::SaveClip)
        };
        let label_style = TextStyle::new(13.5, Weight::SemiBold);
        let label_w = ui.measure(label, label_style).0 + 26.0;
        let label_h = ui.measure("Ag", label_style).1;
        let start = button.x + (button.w - label_w) / 2.0;
        ui.icon(
            ICON_SAVE,
            start + 8.0,
            y + button_h / 2.0,
            16.0,
            label_color,
        );
        ui.text(
            label,
            start + 26.0,
            y + (button_h - label_h) / 2.0,
            label_style,
            label_color,
        );
        y += button_h + 8.0;

        // Folder: where files go, with buttons to open it or choose another.
        let path = fit_text(ui, &r.folder, hint_style, inner_w);
        ui.text(&path, inner_x, y, hint_style, t.text_dim);
        y += 16.0 + 6.0;
        let half = (inner_w - 8.0) / 2.0;
        let cy = y + folder_h / 2.0;
        let buttons = [
            (Hit::ReplayFolder, Key::OpenFolder, inner_x, true),
            (
                Hit::ReplayChangeFolder,
                Key::ChangeFolder,
                inner_x + half + 8.0,
                false,
            ),
        ];
        for (hit, key, bx, with_icon) in buttons {
            let rect = Rect::new(bx, y, half, folder_h);
            self.register(hit, rect);
            let (hover, press) = self.amounts(hit);
            ui.rect(rect, 10.0, t.segment_track);
            let alpha = t.hover.a * (hover + 0.9 * press);
            if alpha > 0.002 {
                ui.rect(rect, 10.0, t.hover.with_alpha(alpha.min(1.0)));
            }
            let label = tr(lang, key);
            let label_w = ui.measure(label, BODY).0;
            let icon_w = if with_icon { 22.0 } else { 0.0 };
            let start = rect.x + (rect.w - label_w - icon_w) / 2.0;
            if with_icon {
                ui.icon(ICON_FOLDER, start + 8.0, cy, 15.0, t.text_dim);
            }
            ui.text(label, start + icon_w, cy - line_h / 2.0, BODY, t.text);
        }
    }
}
