//! What the program does on a system it has no adapters for yet: it says so.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use dockview_domain::i18n::Lang;
use dockview_domain::presenter::{PresentError, Presenter};
use dockview_domain::snap::SnapJob;
use winit::window::{Window, WindowAttributes};

use super::NativeWindow;

const UNSUPPORTED: &str = "this system is not supported yet";

pub fn native_window(_: &Window) -> Option<NativeWindow> {
    None
}

pub fn create_presenter(_: &Window) -> Result<Box<dyn Presenter>, PresentError> {
    Err(PresentError::new(UNSUPPORTED))
}

pub fn style_window(attributes: WindowAttributes) -> WindowAttributes {
    attributes.with_decorations(false)
}

pub fn set_window_frame(_: &Window, _fullscreen: bool) {}

pub fn renderer_name() -> &'static str {
    ""
}

pub fn system_animations_enabled() -> bool {
    true
}

pub fn exclude_from_capture(_: NativeWindow, _: bool) {}

/// The text on the clipboard (nothing is read on this system yet).
pub fn clipboard_text() -> Option<String> {
    None
}

pub fn pick_folder(_: Option<NativeWindow>, _: &Path) -> Option<PathBuf> {
    None
}

pub fn reveal_file(_: &Path) {}

pub fn show_error_box(title: &str, text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{title}: {text}");
}

pub fn default_log_path() -> Option<PathBuf> {
    None
}

pub fn take_screenshot(_: &SnapJob, _: Lang) -> Result<String, String> {
    Err(UNSUPPORTED.into())
}

pub fn screenshots_folder() -> Option<PathBuf> {
    None
}

/// The date and time in UTC (there is no time zone support here yet).
pub fn local_time() -> ((u16, u16, u16), (u16, u16, u16)) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    super::clock::civil(secs)
}
