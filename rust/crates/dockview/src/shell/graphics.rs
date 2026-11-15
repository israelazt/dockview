//! Making the window's graphics: the one place the shell picks the graphics
//! system for the operating system it runs on. Everything else in the shell
//! talks to the `Presenter` port.

use dockview_domain::presenter::{PresentError, Presenter};
use winit::window::Window;

/// The graphics for `window`, as big as it is now.
#[cfg(windows)]
pub fn create_presenter(window: &Window) -> Result<Box<dyn Presenter>, PresentError> {
    use std::ffi::c_void;

    use dockview_platform::presenter::D3dPresenter;
    use windows::Win32::Foundation::HWND;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let hwnd = match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => HWND(h.hwnd.get() as *mut c_void),
        _ => return Err(PresentError::new("the window has no Win32 handle")),
    };
    let size = window.inner_size();
    D3dPresenter::new(hwnd, size.width, size.height).map(|p| Box::new(p) as Box<dyn Presenter>)
}
