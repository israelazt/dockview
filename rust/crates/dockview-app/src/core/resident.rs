//! The app's side of living in the tray: the capture hotkey and the icon.

use dockview_domain::i18n::Lang;

use super::*;
use crate::resident_service::ResidentEvent;

impl Core {
    /// Whether closing the window should leave Dockview in the tray instead of
    /// quitting (the icon is there to bring it back).
    pub fn keeps_in_tray(&self) -> bool {
        self.resident.keeps_in_tray()
    }

    /// What the user chose about screenshots.
    pub fn capture_settings(&self) -> &dockview_domain::capture_settings::CaptureSettings {
        &self.settings.capture
    }

    /// The language the interface is in now.
    pub fn ui_lang(&self) -> Lang {
        self.resident_lang()
    }

    /// Tells the user what came of a screenshot (a toast when the window shows).
    pub fn report(&mut self, event: dockview_domain::events::AppEvent) {
        self.emit(event);
    }

    fn resident_lang(&self) -> Lang {
        self.lang_mode().resolve(self.view.system_lang)
    }

    /// Keeps the hotkey and the tray in line with the settings and does what
    /// they report: start a screenshot, show the window, quit.
    pub(super) fn maintain_resident(&mut self) {
        if self.cli.ui_preview {
            return; // a preview touches nothing outside its window
        }
        let lang = self.resident_lang();
        for event in self.resident.sync(&self.settings.capture, lang) {
            self.emit(event);
        }
        for event in self.resident.poll(self.settings.capture.mode) {
            match event {
                ResidentEvent::Capture(mode) => self.host.start_capture(mode),
                ResidentEvent::OpenWindow => self.host.show_window(),
                ResidentEvent::Quit => self.host.quit(),
            }
        }
    }
}
