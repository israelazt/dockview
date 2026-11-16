//! The Settings mode of `Core`: what it shows and what each of its choices
//! does. It is the only place that changes these settings from the interface.

use dockview_domain::settings_view::{SettingsAction, SettingsView};

use super::*;

impl Core {
    /// What the Settings mode shows.
    pub(super) fn settings_view(&self) -> SettingsView {
        SettingsView {
            preset: self.settings.render_preset,
            video_mode: self.settings.video_mode,
            theme_mode: self.theme_mode(),
            lang_mode: self.lang_mode(),
            keep_in_tray: self.settings.capture.keep_in_tray,
        }
    }

    /// Carries out a choice of the Settings mode.
    pub(super) fn apply_settings(&mut self, change: SettingsAction) {
        match change {
            SettingsAction::SetVideoMode(mode) => {
                self.settings.video_mode = mode;
                // A deliberate choice ends any automatic switch and allows a
                // new automatic attempt later (for example after changing port).
                self.measures.auto.reset();
                self.select_video(self.sources.video_index);
            }
            SettingsAction::SetPreset(preset) => {
                self.settings.render_preset = preset;
                self.host.set_render_preset(preset);
            }
            SettingsAction::SetTheme(mode) => {
                self.settings.theme = mode;
                self.cli.theme = None; // an explicit choice wins over --theme
            }
            SettingsAction::SetLang(mode) => {
                self.settings.language = mode;
                self.cli.lang = None;
            }
            SettingsAction::SetKeepInTray(on) => {
                self.settings.capture.keep_in_tray = on;
            }
        }
    }
}
