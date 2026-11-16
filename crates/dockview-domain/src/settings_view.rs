//! What the Settings mode shows and asks for. The mode owns its own view and
//! its own actions: nothing of the viewer or the Studio is read or asked for
//! from there, and the other modes do not touch these.

use crate::settings::{LangMode, RenderPreset, ThemeMode, VideoModePref};

/// What the Settings mode displays.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsView {
    /// How the picture is scaled (image quality).
    pub preset: RenderPreset,
    /// How the capture mode is chosen.
    pub video_mode: VideoModePref,
    pub theme_mode: ThemeMode,
    pub lang_mode: LangMode,
    /// Closing the window leaves the app in the tray (so the replay and the
    /// hotkeys keep working) instead of quitting.
    pub keep_in_tray: bool,
}

/// A change asked for in the Settings mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    SetPreset(RenderPreset),
    SetVideoMode(VideoModePref),
    SetTheme(ThemeMode),
    SetLang(LangMode),
    SetKeepInTray(bool),
}
