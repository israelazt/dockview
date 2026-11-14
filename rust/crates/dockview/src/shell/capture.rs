//! Taking a screenshot from the window's side: the settings decide what, where
//! and how; the overlay (which needs the screen and the interface engine) is
//! run here, and what came of it is told to the application.

use dockview_domain::events::AppEvent;
use dockview_domain::select::Mode;
use dockview_platform::screen_io::{local_time, pictures_screenshots_folder};

use super::*;

impl Shell {
    /// Starts a screenshot in `mode`. The selection runs its own short loop
    /// over the whole screen, so the window waits until it is done.
    pub(super) fn start_capture(&mut self, mode: Mode) {
        let settings = self.core.capture_settings().clone();
        let folder = pictures_screenshots_folder().unwrap_or_else(|| std::path::PathBuf::from("."));
        let (date, time) = local_time();
        let job = settings.job(mode, &folder, date, time);
        let lang = self.core.ui_lang();
        match crate::snap::take(&job, lang) {
            Ok(done) if done == dockview_app::snap_service::CANCELLED => {}
            Ok(done) => {
                dockview_domain::log_info!("screenshot: {done}");
                self.core.report(if job.out.is_some() {
                    AppEvent::ScreenshotSaved
                } else {
                    AppEvent::ScreenshotCopied
                });
            }
            Err(e) => {
                dockview_domain::log_error!("screenshot: {e}");
                self.core.report(AppEvent::ScreenshotFailed);
            }
        }
        self.needs_redraw = true;
    }

    /// Shows the window (it may be hidden in the tray) and brings it forward.
    pub(super) fn show_window(&mut self) {
        if let Some(window) = &self.window {
            window.set_visible(true);
            window.set_minimized(false);
            window.focus_window();
        }
        self.hidden = false;
        self.needs_redraw = true;
    }

    /// Hides the window and leaves Dockview in the tray.
    pub(super) fn hide_window(&mut self) {
        if let Some(window) = &self.window {
            window.set_visible(false);
        }
        self.hidden = true;
    }
}
