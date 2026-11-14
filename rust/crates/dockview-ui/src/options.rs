//! The choices the interface offers, in the order they are listed, shared by
//! the controls (the settings popover) and the Studio panel.

use dockview_domain::settings::{LangMode, RenderPreset, VideoModePref};
use dockview_domain::theme::ThemeMode;

pub(crate) const PRESETS: [RenderPreset; 3] = [
    RenderPreset::Performance,
    RenderPreset::Balanced,
    RenderPreset::Quality,
];
pub(crate) const THEMES: [ThemeMode; 3] = [ThemeMode::Auto, ThemeMode::Dark, ThemeMode::Light];
pub(crate) const LANGS: [LangMode; 3] = [LangMode::Auto, LangMode::En, LangMode::Es];
pub(crate) const VIDEO_MODES: [VideoModePref; 2] = [VideoModePref::Auto, VideoModePref::Compressed];
