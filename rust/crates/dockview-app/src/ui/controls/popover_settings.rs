//! The settings popover: devices, image, theme, language and statistics.

use super::*;

impl Controls {
    pub(super) fn draw_popover(&mut self, ui: &mut Ui, m: &Model, pill: Rect) {
        let t = &m.theme;
        let height = popover_height(m);
        let (screen_w, _) = ui.logical_size();
        let x = (pill.x + pill.w - POPOVER_WIDTH)
            .min(screen_w - POPOVER_WIDTH - POPOVER_GAP)
            .max(POPOVER_GAP)
            .round();
        let y = (pill.y - POPOVER_GAP - height).max(POPOVER_GAP).round();
        let rect = Rect::new(x, y, POPOVER_WIDTH, height);
        self.register(Hit::Popover, rect);

        ui.shadow(rect, 20.0, 26.0, t.shadow);
        ui.panel(rect, 20.0, t.surface, t.border, 1.0);

        let inner_x = x + POPOVER_PAD;
        let inner_w = POPOVER_WIDTH - POPOVER_PAD * 2.0;
        let mut y = y + POPOVER_PAD;

        // Indicator positions: start at the current choice, then slide.
        let targets = [
            PRESETS.iter().position(|v| *v == m.preset).unwrap_or(1) as f32,
            THEMES.iter().position(|v| *v == m.theme_mode).unwrap_or(0) as f32,
            LANGS.iter().position(|v| *v == m.lang_mode).unwrap_or(0) as f32,
            VIDEO_MODES
                .iter()
                .position(|v| *v == m.video_mode)
                .unwrap_or(0) as f32,
        ];
        let mut slide = self.segments.unwrap_or(targets);
        for (position, target) in slide.iter_mut().zip(targets) {
            *position = self.approach(*position, target, SLIDE_RATE);
        }
        self.segments = Some(slide);

        y = self.device_section(
            ui,
            m,
            Key::Video,
            &m.video_devices,
            m.video_selected,
            Hit::Video,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;
        y = self.device_section(
            ui,
            m,
            Key::Audio,
            &m.audio_devices,
            m.audio_selected,
            Hit::Audio,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;
        y = self.device_section(
            ui,
            m,
            Key::Microphone,
            &m.mic_devices,
            m.mic_selected,
            Hit::MicDevice,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;

        let lang = m.lang;
        let modes = [tr(lang, Key::Auto), tr(lang, Key::ModeCompressed)];
        let mode_selected = VIDEO_MODES
            .iter()
            .position(|v| *v == m.video_mode)
            .unwrap_or(0);
        y = self.segmented_section(
            ui,
            m,
            Key::VideoMode,
            &modes,
            mode_selected,
            slide[3],
            Hit::VideoMode,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;
        let presets = [
            tr(lang, Key::PresetPerformance),
            tr(lang, Key::PresetBalanced),
            tr(lang, Key::PresetHigh),
        ];
        let preset_selected = PRESETS.iter().position(|p| *p == m.preset).unwrap_or(1);
        y = self.segmented_section(
            ui,
            m,
            Key::ImageQuality,
            &presets,
            preset_selected,
            slide[0],
            Hit::Preset,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;

        let themes = [
            tr(lang, Key::Auto),
            tr(lang, Key::Dark),
            tr(lang, Key::Light),
        ];
        let theme_selected = THEMES.iter().position(|v| *v == m.theme_mode).unwrap_or(0);
        y = self.segmented_section(
            ui,
            m,
            Key::Theme,
            &themes,
            theme_selected,
            slide[1],
            Hit::Theme,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;

        let langs = [
            tr(lang, Key::Auto),
            tr(lang, Key::English),
            tr(lang, Key::Spanish),
        ];
        let lang_selected = LANGS.iter().position(|v| *v == m.lang_mode).unwrap_or(0);
        y = self.segmented_section(
            ui,
            m,
            Key::Language,
            &langs,
            lang_selected,
            slide[2],
            Hit::Lang,
            inner_x,
            y,
            inner_w,
        );
        y += SECTION_GAP;
        self.stats_row(ui, m, inner_x, y, inner_w);
    }

    /// The row that toggles the statistics panel.
    pub(super) fn stats_row(&mut self, ui: &mut Ui, m: &Model, x: f32, y: f32, w: f32) {
        let t = &m.theme;
        let row = Rect::new(x, y, w, ROW_HEIGHT);
        self.register(Hit::Stats, row);
        let (hover, press) = self.amounts(Hit::Stats);
        let alpha = t.hover.a * (hover + 0.9 * press);
        if alpha > 0.002 {
            ui.rect(row, 9.0, t.hover.with_alpha(alpha.min(1.0)));
        }
        let cy = y + ROW_HEIGHT / 2.0;
        ui.icon(ICON_ACTIVITY, x + 20.0, cy, 15.0, t.text_dim);
        let line_h = ui.measure("Ag", BODY).1;
        ui.text(
            tr(m.lang, Key::Statistics),
            x + 38.0,
            cy - line_h / 2.0,
            BODY,
            t.text,
        );
        if m.stats_open {
            ui.icon(ICON_CHECK, x + w - 18.0, cy, 15.0, t.accent);
        } else {
            let hint = TextStyle::new(12.0, Weight::Medium);
            let key_w = ui.measure("I", hint).0 + 12.0;
            let key = Rect::new(x + w - 10.0 - key_w, cy - 10.0, key_w, 20.0);
            ui.rect(key, 6.0, t.segment_track);
            ui.text("I", key.x + 6.0, cy - 9.0, hint, t.text_dim);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn device_section(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        title: Key,
        devices: &[String],
        selected: Option<usize>,
        hit: fn(usize) -> Hit,
        x: f32,
        y: f32,
        w: f32,
    ) -> f32 {
        self.list_section(
            ui,
            m,
            title,
            devices,
            selected,
            hit,
            x,
            y,
            w,
            MAX_LISTED_DEVICES,
        )
    }
}

/// Height of the settings popover for `m`.
pub(super) fn popover_height(m: &Model) -> f32 {
    let list_rows = |n: usize| n.clamp(1, MAX_LISTED_DEVICES) as f32 * ROW_HEIGHT;
    let device = |n: usize| LABEL_HEIGHT + list_rows(n);
    let segmented = LABEL_HEIGHT + ROW_HEIGHT;
    POPOVER_PAD * 2.0
        + device(m.video_devices.len())
        + device(m.audio_devices.len())
        + device(m.mic_devices.len())
        + segmented * 4.0
        + SECTION_GAP * 7.0
        + ROW_HEIGHT
}
