//! The settings popover: devices, image, theme, language and statistics.

use super::*;

/// One list of devices in the popover.
struct DeviceList<'a> {
    title: Key,
    devices: &'a [String],
    selected: Option<usize>,
    hit: fn(usize) -> Hit,
}

/// One row of segmented choices (for example light / dark / auto).
struct Choice<'a> {
    title: Key,
    labels: &'a [&'a str],
    selected: usize,
    /// Where the sliding indicator is now (it glides to `selected`).
    position: f32,
    hit: fn(usize) -> Hit,
}

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
        let slide = self.slide_selections(m);
        let mut y = y + POPOVER_PAD;
        y = self.device_sections(ui, m, inner_x, y, inner_w);
        y = self.option_sections(ui, m, slide, inner_x, y, inner_w);
        self.stats_row(ui, m, inner_x, y, inner_w);
    }

    /// Where each segmented choice's indicator is now: it starts at the current
    /// choice, then slides to a new one. Order: preset, theme, language, mode.
    fn slide_selections(&mut self, m: &Model) -> [f32; 4] {
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
        slide
    }

    /// The three device lists: video, audio and microphone. Returns the y after
    /// the last one, with its gap.
    fn device_sections(&mut self, ui: &mut Ui, m: &Model, x: f32, mut y: f32, w: f32) -> f32 {
        let lists = [
            DeviceList {
                title: Key::Video,
                devices: &m.video_devices,
                selected: m.video_selected,
                hit: Hit::Video,
            },
            DeviceList {
                title: Key::Audio,
                devices: &m.audio_devices,
                selected: m.audio_selected,
                hit: Hit::Audio,
            },
            DeviceList {
                title: Key::Microphone,
                devices: &m.mic_devices,
                selected: m.mic_selected,
                hit: Hit::MicDevice,
            },
        ];
        for list in lists {
            y = self.device_section(
                ui,
                m,
                list.title,
                list.devices,
                list.selected,
                list.hit,
                x,
                y,
                w,
            );
            y += SECTION_GAP;
        }
        y
    }

    /// The segmented choices: video mode, image quality, theme and language.
    /// Returns the y after the last one, with its gap.
    fn option_sections(
        &mut self,
        ui: &mut Ui,
        m: &Model,
        slide: [f32; 4],
        x: f32,
        mut y: f32,
        w: f32,
    ) -> f32 {
        let lang = m.lang;
        let modes = [tr(lang, Key::Auto), tr(lang, Key::ModeCompressed)];
        let presets = [
            tr(lang, Key::PresetPerformance),
            tr(lang, Key::PresetBalanced),
            tr(lang, Key::PresetHigh),
        ];
        let themes = [
            tr(lang, Key::Auto),
            tr(lang, Key::Dark),
            tr(lang, Key::Light),
        ];
        let langs = [
            tr(lang, Key::Auto),
            tr(lang, Key::English),
            tr(lang, Key::Spanish),
        ];
        let sections = [
            Choice {
                title: Key::VideoMode,
                labels: &modes,
                selected: VIDEO_MODES
                    .iter()
                    .position(|v| *v == m.video_mode)
                    .unwrap_or(0),
                position: slide[3],
                hit: Hit::VideoMode,
            },
            Choice {
                title: Key::ImageQuality,
                labels: &presets,
                selected: PRESETS.iter().position(|p| *p == m.preset).unwrap_or(1),
                position: slide[0],
                hit: Hit::Preset,
            },
            Choice {
                title: Key::Theme,
                labels: &themes,
                selected: THEMES.iter().position(|v| *v == m.theme_mode).unwrap_or(0),
                position: slide[1],
                hit: Hit::Theme,
            },
            Choice {
                title: Key::Language,
                labels: &langs,
                selected: LANGS.iter().position(|v| *v == m.lang_mode).unwrap_or(0),
                position: slide[2],
                hit: Hit::Lang,
            },
        ];
        for choice in sections {
            y = self.segmented_section(
                ui,
                m,
                choice.title,
                choice.labels,
                choice.selected,
                choice.position,
                choice.hit,
                x,
                y,
                w,
            );
            y += SECTION_GAP;
        }
        y
    }

    /// The row that toggles the statistics panel.
    pub(super) fn stats_row(&mut self, ui: &mut Ui, m: &Model, x: f32, y: f32, w: f32) {
        let t = &m.theme;
        let row = Rect::new(x, y, w, ROW_HEIGHT);
        self.register(Hit::Stats, row);
        self.wash(ui, m, Hit::Stats, row, 9.0, 1.0);
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
