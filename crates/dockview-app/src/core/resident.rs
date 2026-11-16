//! The app's side of living in the tray: the capture hotkey and the icon.

use dockview_domain::events::AppEvent;
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
                ResidentEvent::Capture(mode) => {
                    if dockview_domain::snap::ENABLED {
                        self.host.start_capture(mode);
                    }
                }
                ResidentEvent::OpenWindow => self.host.show_window(),
                ResidentEvent::NotificationClicked => self.reveal_last_screenshot(),
                ResidentEvent::Quit => self.host.quit(),
            }
        }
    }
}

impl Core {
    /// A screenshot was taken: `saved` is the file it went to (if it was saved)
    /// and `copied` whether it is on the clipboard. The user is told with the
    /// toast of the window, or, with the window hidden in the tray, with a
    /// notification from the icon (a click on it shows the file).
    pub fn screenshot_taken(&mut self, saved: Option<std::path::PathBuf>, copied: bool) {
        use dockview_domain::i18n::{tr, Key};
        let lang = self.resident_lang();
        let event = match (&saved, copied) {
            (Some(_), _) => AppEvent::ScreenshotSaved,
            (None, _) => AppEvent::ScreenshotCopied,
        };
        if self.view.window_hidden {
            let title = tr(
                lang,
                if saved.is_some() && copied {
                    Key::ToastScreenshotSavedCopied
                } else if saved.is_some() {
                    Key::ToastScreenshotSaved
                } else {
                    Key::ToastScreenshotCopied
                },
            );
            let text = saved
                .as_deref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.resident.notify(title, &text);
        } else {
            self.emit(event);
        }
        self.view.last_screenshot = saved;
    }

    /// The window is hidden in the tray (or shown again).
    pub fn set_window_hidden(&mut self, hidden: bool) {
        self.view.window_hidden = hidden;
    }

    /// Shows the last screenshot in the file manager, if it was saved.
    pub(super) fn reveal_last_screenshot(&mut self) {
        if let Some(path) = self.view.last_screenshot.clone() {
            self.host.reveal_file(&path);
        }
    }
}
