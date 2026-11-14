//! Windows' side of the screenshot ports: grabbing the screen with GDI and
//! writing pictures with WIC (see `snapshot` and `imagefile`).

use std::ffi::c_void;
use std::path::Path;

use dockview_domain::desktop::{Desktop, PxRect};
use dockview_domain::image::{Image, ImageFormat};
use dockview_domain::ports::{ImageOutput, ScreenGrabber};
use dockview_domain::sources::WindowInfo;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};

use crate::{imagefile, snapshot, targets};

/// The real screen of this PC.
pub struct PlatformScreen;

impl ScreenGrabber for PlatformScreen {
    fn desktop(&self) -> Desktop {
        snapshot::desktop()
    }

    fn grab_rect(&self, rect: PxRect, cursor: bool) -> Result<Image, String> {
        snapshot::capture_rect(rect, cursor)
    }

    fn grab_window(&self, handle: isize, cursor: bool) -> Result<Image, String> {
        snapshot::capture_window(handle, cursor)
    }

    fn windows(&self) -> Vec<WindowInfo> {
        targets::list_windows()
    }

    fn window_rect(&self, handle: isize) -> Option<PxRect> {
        let hwnd = HWND(handle as *mut c_void);
        let mut frame = RECT::default();
        // SAFETY: the attribute is a RECT, as the size passed says.
        unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut frame as *mut RECT).cast::<c_void>(),
                std::mem::size_of::<RECT>() as u32,
            )
        }
        .ok()?;
        let (w, h) = (frame.right - frame.left, frame.bottom - frame.top);
        (w > 0 && h > 0).then(|| PxRect::new(frame.left, frame.top, w as u32, h as u32))
    }
}

impl ImageOutput for PlatformScreen {
    fn save(&self, image: &Image, path: &Path, format: ImageFormat) -> Result<(), String> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("could not create the folder: {e}"))?;
        }
        imagefile::save(image, path, format)
    }

    fn copy(&self, image: &Image) -> Result<(), String> {
        imagefile::copy_to_clipboard(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_real_screen_answers_through_the_ports() {
        let screen = PlatformScreen;
        let desktop = screen.desktop();
        if desktop.monitors.is_empty() {
            return; // no display in this session
        }
        let rect = desktop.monitors[0].bounds;
        let image = screen
            .grab_rect(PxRect::new(rect.x, rect.y, 20, 10), false)
            .unwrap();
        assert_eq!((image.width, image.height), (20, 10));
        // The windows come with rectangles that are on the virtual screen.
        for window in screen.windows().iter().take(3) {
            if let Some(r) = screen.window_rect(window.handle) {
                assert!(!r.is_empty());
            }
        }
        assert!(screen.window_rect(0x1234_5678).is_none());
    }

    #[test]
    fn saving_creates_the_folder() {
        let dir = std::env::temp_dir().join("dockview-screen-io").join("deep");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("x.png");
        PlatformScreen
            .save(&Image::solid(4, 4, [1, 2, 3, 255]), &path, ImageFormat::Png)
            .unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
