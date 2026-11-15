//! The Windows implementation of [`super`].

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use dockview_domain::i18n::Lang;
use dockview_domain::presenter::{PresentError, Presenter};
use dockview_domain::snap::SnapJob;
use dockview_platform::presenter::D3dPresenter;
use dockview_platform::screen_io::{local_time as clock, pictures_screenshots_folder};
use dockview_platform::{clips, log_file};
use windows::core::{BOOL, HSTRING};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, SetWindowDisplayAffinity, SystemParametersInfoW, MB_ICONERROR, MB_OK,
    SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WDA_EXCLUDEFROMCAPTURE,
    WDA_NONE,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowAttributes};

use super::NativeWindow;

fn hwnd_of(window: NativeWindow) -> HWND {
    HWND(window.0 as *mut c_void)
}

/// The native handle of `window`.
pub fn native_window(window: &Window) -> Option<NativeWindow> {
    match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => Some(NativeWindow(h.hwnd.get())),
        _ => None,
    }
}

/// The graphics for `window`, as big as it is now.
pub fn create_presenter(window: &Window) -> Result<Box<dyn Presenter>, PresentError> {
    let Some(native) = native_window(window) else {
        return Err(PresentError::new("the window has no Win32 handle"));
    };
    let size = window.inner_size();
    D3dPresenter::new(hwnd_of(native), size.width, size.height)
        .map(|p| Box::new(p) as Box<dyn Presenter>)
}

/// What the window is made like: no system title bar (the program draws its
/// own), but the shadow Windows gives a window with a frame.
pub fn style_window(attributes: WindowAttributes) -> WindowAttributes {
    use winit::platform::windows::WindowAttributesExtWindows;
    attributes
        .with_decorations(false)
        .with_undecorated_shadow(true)
}

/// Rounds the corners of the window as Windows 11 does for every window.
pub fn round_corners(window: &Window) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };
    let Some(native) = native_window(window) else {
        return;
    };
    let preference = DWMWCP_ROUND;
    // SAFETY: the pointer refers to a live value of the size the attribute
    // expects; older Windows versions just do not know the attribute.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd_of(native),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&preference as *const _) as *const c_void,
            std::mem::size_of_val(&preference) as u32,
        );
    }
}

/// The name of the graphics API the window draws with, for the title bar.
pub fn renderer_name() -> &'static str {
    "D3D11"
}

/// Whether Windows allows UI animations ("Show animations in Windows").
pub fn system_animations_enabled() -> bool {
    let mut enabled = BOOL(1);
    // SAFETY: the out-pointer refers to a live BOOL, as SPI_GETCLIENTAREAANIMATION requires.
    let result = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    result.map_or(true, |()| enabled.as_bool())
}

/// Hides (or shows again) the window from screen captures.
pub fn exclude_from_capture(window: NativeWindow, on: bool) {
    let affinity = if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
    // SAFETY: a plain call on our own window; it fails harmlessly on Windows
    // versions that do not support it.
    unsafe {
        let _ = SetWindowDisplayAffinity(hwnd_of(window), affinity);
    }
}

/// Lets the user pick a folder, starting at `start`.
pub fn pick_folder(owner: Option<NativeWindow>, start: &Path) -> Option<PathBuf> {
    clips::pick_folder(owner.map(hwnd_of), start)
}

/// Shows `path` in the file manager, selected.
pub fn reveal_file(path: &Path) {
    clips::reveal_file(path);
}

/// A box with an error, for when the program cannot even open its window.
pub fn show_error_box(title: &str, text: &str) {
    // SAFETY: a plain call with strings that outlive it and no owner window.
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(title),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// Where the log is written.
pub fn default_log_path() -> Option<PathBuf> {
    log_file::default_log_path()
}

/// Takes the screenshot `job` asks for (after its delay): what was done, or
/// `snap_service::CANCELLED`.
pub fn take_screenshot(job: &SnapJob, lang: Lang) -> Result<String, String> {
    crate::snap::take(job, lang)
}

/// The folder screenshots go to unless the user chose another.
pub fn screenshots_folder() -> Option<PathBuf> {
    pictures_screenshots_folder()
}

/// The local date and time, `((year, month, day), (hour, minute, second))`.
pub fn local_time() -> ((u16, u16, u16), (u16, u16, u16)) {
    clock()
}
