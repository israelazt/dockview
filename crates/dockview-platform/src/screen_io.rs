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

/// The folder screenshots go to unless the user chose another: the Screenshots
/// folder inside Pictures (the same one Windows' own tool uses).
pub fn pictures_screenshots_folder() -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Pictures, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    // SAFETY: the returned string is read and then freed as the API requires.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Pictures, KF_FLAG_DEFAULT, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0.cast()));
        text.map(|p| std::path::PathBuf::from(p).join("Screenshots"))
    }
}

/// The local date and time, as `((year, month, day), (hour, minute, second))`.
pub fn local_time() -> ((u16, u16, u16), (u16, u16, u16)) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: a plain query with no arguments.
    let t = unsafe { GetLocalTime() };
    ((t.wYear, t.wMonth, t.wDay), (t.wHour, t.wMinute, t.wSecond))
}

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
    fn the_screenshots_folder_is_inside_pictures_and_the_clock_is_sane() {
        let folder = pictures_screenshots_folder().expect("a Pictures folder");
        assert!(folder.ends_with("Screenshots"), "{folder:?}");
        let ((year, month, day), (hour, minute, second)) = local_time();
        assert!(year >= 2024 && (1..=12).contains(&month) && (1..=31).contains(&day));
        assert!(hour < 24 && minute < 60 && second < 61);
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
