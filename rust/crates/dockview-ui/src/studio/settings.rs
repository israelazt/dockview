//! The Settings section: image quality, video mode, theme and language, the
//! same choices (and the same actions) as the settings popover.

use dockview_domain::i18n::{tr, Key, Lang};
use dockview_domain::view::{Action, Model};

use super::widgets::{GAP, LABEL_H, ROW_H};
use super::{StudioPanel, PAD};
use crate::options::{LANGS, PRESETS, THEMES, VIDEO_MODES};
use crate::Rect;

/// Space above the first control and below the last.
const PAD_TOP: f32 = 4.0;
const PAD_BOTTOM: f32 = 14.0;
/// How many groups of choices there are.
const GROUPS: f32 = 4.0;

/// How tall the body is: four titled rows with a gap between them.
pub fn body_height() -> f32 {
    PAD_TOP + GROUPS * (LABEL_H + ROW_H) + (GROUPS - 1.0) * GAP + PAD_BOTTOM
}

/// The words of the three choices of the image quality.
fn quality_labels(lang: Lang) -> [&'static str; 3] {
    [
        tr(lang, Key::PresetPerformance),
        tr(lang, Key::PresetBalanced),
        tr(lang, Key::PresetHigh),
    ]
}

/// Where `current` is among `all`; the first when it is not there.
fn position_of<T: PartialEq>(all: &[T], current: T) -> usize {
    all.iter().position(|v| *v == current).unwrap_or(0)
}

impl StudioPanel {
    /// Draws the body of the Settings section with its top-left corner at the
    /// corner of `body`.
    pub(super) fn draw_settings(&mut self, ui: &mut crate::Ui, m: &Model, body: Rect) {
        let t = &m.theme;
        let lang = m.lang;
        let (x, w) = (body.x + PAD, body.w - PAD * 2.0);
        let mut y = body.y + PAD_TOP;

        let auto = tr(lang, Key::Auto);

        y += self.caption(ui, t, tr(lang, Key::ImageQuality), x, y);
        y += self.segmented(
            ui,
            t,
            "quality",
            &quality_labels(lang),
            position_of(&PRESETS, m.preset),
            PRESETS.iter().map(|p| Action::SetPreset(*p)).collect(),
            x,
            y,
            w,
        ) + GAP;

        y += self.caption(ui, t, tr(lang, Key::VideoMode), x, y);
        y += self.segmented(
            ui,
            t,
            "video-mode",
            &[auto, tr(lang, Key::ModeCompressed)],
            position_of(&VIDEO_MODES, m.video_mode),
            VIDEO_MODES
                .iter()
                .map(|v| Action::SetVideoMode(*v))
                .collect(),
            x,
            y,
            w,
        ) + GAP;

        y += self.caption(ui, t, tr(lang, Key::Theme), x, y);
        y += self.segmented(
            ui,
            t,
            "theme",
            &[auto, tr(lang, Key::Dark), tr(lang, Key::Light)],
            position_of(&THEMES, m.theme_mode),
            THEMES.iter().map(|v| Action::SetTheme(*v)).collect(),
            x,
            y,
            w,
        ) + GAP;

        y += self.caption(ui, t, tr(lang, Key::Language), x, y);
        self.segmented(
            ui,
            t,
            "language",
            &[auto, tr(lang, Key::English), tr(lang, Key::Spanish)],
            position_of(&LANGS, m.lang_mode),
            LANGS.iter().map(|v| Action::SetLang(*v)).collect(),
            x,
            y,
            w,
        );
    }
}
