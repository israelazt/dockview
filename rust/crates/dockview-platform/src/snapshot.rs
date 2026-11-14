//! Still pictures of the screen: a rectangle of the virtual screen, a monitor
//! or a window, as an [`Image`]. This is the capture half of the screenshot
//! tool (see `SNIP_PLAN.md`).
//!
//! It uses GDI, which gives exactly what is on the screen in full colour, with
//! no GPU device to set up (a still does not need the video path). A window
//! that is covered by others is still captured whole: `PrintWindow` asks the
//! window to draw itself.

use std::ffi::c_void;

use dockview_domain::desktop::{Desktop, DesktopMonitor, PxRect};
use dockview_domain::image::Image;
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EnumDisplayMonitors,
    GetDC, GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    CAPTUREBLT, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITORINFO, MONITORINFOEXW,
    SRCCOPY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    DrawIconEx, GetCursorInfo, GetIconInfo, GetWindowRect, CURSORINFO, CURSOR_SHOWING, DI_NORMAL,
    ICONINFO,
};

/// `PW_RENDERFULLCONTENT`: also draws windows that DWM composes (most apps).
const PW_RENDERFULLCONTENT: u32 = 0x2;

// ---------------------------------------------------------------------------
// The desktop
// ---------------------------------------------------------------------------

unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<DesktopMonitor>` passed to EnumDisplayMonitors.
    let list = unsafe { &mut *(data.0 as *mut Vec<DesktopMonitor>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `info` is a valid MONITORINFOEXW with cbSize set.
    let ok = unsafe {
        GetMonitorInfoW(
            monitor,
            (&mut info as *mut MONITORINFOEXW).cast::<MONITORINFO>(),
        )
    }
    .as_bool();
    if ok {
        let r = info.monitorInfo.rcMonitor;
        let name_len = info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szDevice.len());
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        // SAFETY: plain query with valid out pointers; on failure 96 stays.
        let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
        list.push(DesktopMonitor {
            bounds: PxRect::new(
                r.left,
                r.top,
                (r.right - r.left).max(0) as u32,
                (r.bottom - r.top).max(0) as u32,
            ),
            name: String::from_utf16_lossy(&info.szDevice[..name_len]),
            primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
            scale_percent: dpi_x * 100 / 96,
        });
    }
    BOOL(1)
}

/// The monitors as they sit on the virtual screen, the primary one first.
pub fn desktop() -> Desktop {
    let mut monitors: Vec<DesktopMonitor> = Vec::new();
    // SAFETY: the callback only touches `monitors`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_callback),
            LPARAM(&mut monitors as *mut Vec<DesktopMonitor> as isize),
        );
    }
    monitors.sort_by_key(|m| !m.primary);
    Desktop::new(monitors)
}

// ---------------------------------------------------------------------------
// Drawing into a bitmap
// ---------------------------------------------------------------------------

/// A device context with a top-down 32-bit bitmap selected, released on drop.
pub(crate) struct Canvas {
    pub(crate) dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut c_void,
    width: u32,
    height: u32,
}

impl Canvas {
    /// A surface holding the picture `image` (opaque pixels as they are).
    pub(crate) fn from_image(image: &Image) -> Result<Self, String> {
        let canvas = Self::new(image.width, image.height)?;
        if !image.is_valid() {
            return Err("the picture is damaged".into());
        }
        // SAFETY: `bits` holds exactly width * height * 4 bytes, the length of
        // a valid image.
        unsafe {
            std::ptr::copy_nonoverlapping(
                image.bgra.as_ptr(),
                canvas.bits.cast::<u8>(),
                image.bgra.len(),
            );
        }
        Ok(canvas)
    }

    pub(crate) fn new(width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 32_000 || height > 32_000 {
            return Err(format!("a picture of {width}x{height} cannot be made"));
        }
        // SAFETY: documented GDI sequence; every handle is checked and released in Drop.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err("could not create a drawing surface".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // negative: top row first
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| {
                    let _ = DeleteDC(dc);
                    format!("could not create the picture: {e}")
                })?;
            let previous = SelectObject(dc, bitmap.into());
            Ok(Self {
                dc,
                bitmap,
                previous,
                bits,
                width,
                height,
            })
        }
    }

    /// The pixels drawn so far, with the alpha made opaque (GDI leaves it 0).
    pub(crate) fn into_image(self) -> Image {
        let len = self.width as usize * self.height as usize * 4;
        // SAFETY: `bits` points to `len` bytes owned by the DIB section, which
        // lives until `self` is dropped at the end of this function.
        let mut bgra = unsafe { std::slice::from_raw_parts(self.bits as *const u8, len) }.to_vec();
        for px in bgra.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        Image {
            width: self.width,
            height: self.height,
            bgra,
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: the handles were created in `new` and are released once.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Draws the mouse cursor onto `canvas`, if it is visible, where it is on the
/// screen relative to `origin` (the screen position of the canvas' corner).
fn draw_cursor(canvas: &Canvas, origin: (i32, i32)) {
    let mut info = CURSORINFO {
        cbSize: std::mem::size_of::<CURSORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: plain queries and a draw into our own bitmap; the icon bitmaps
    // that GetIconInfo hands over are deleted.
    unsafe {
        if GetCursorInfo(&mut info).is_err() || info.flags.0 & CURSOR_SHOWING.0 == 0 {
            return;
        }
        let mut icon = ICONINFO::default();
        let (mut hot_x, mut hot_y) = (0, 0);
        if GetIconInfo(info.hCursor.into(), &mut icon).is_ok() {
            hot_x = icon.xHotspot as i32;
            hot_y = icon.yHotspot as i32;
            if !icon.hbmMask.is_invalid() {
                let _ = DeleteObject(icon.hbmMask.into());
            }
            if !icon.hbmColor.is_invalid() {
                let _ = DeleteObject(icon.hbmColor.into());
            }
        }
        let POINT { x, y } = info.ptScreenPos;
        let _ = DrawIconEx(
            canvas.dc,
            x - origin.0 - hot_x,
            y - origin.1 - hot_y,
            info.hCursor.into(),
            0,
            0,
            0,
            None,
            DI_NORMAL,
        );
    }
}

// ---------------------------------------------------------------------------
// Stills
// ---------------------------------------------------------------------------

/// What is on `rect` of the virtual screen now, with the mouse cursor drawn on
/// it when `cursor` is set. The rectangle is cut to the virtual screen first.
pub fn capture_rect(rect: PxRect, cursor: bool) -> Result<Image, String> {
    let rect = desktop()
        .clamp(&rect)
        .ok_or("that part is not on any monitor")?;
    let canvas = Canvas::new(rect.w, rect.h)?;
    // SAFETY: the screen DC is released on every path; BitBlt copies into our DIB.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("could not read the screen".into());
        }
        let copied = BitBlt(
            canvas.dc,
            0,
            0,
            rect.w as i32,
            rect.h as i32,
            Some(screen),
            rect.x,
            rect.y,
            SRCCOPY | CAPTUREBLT,
        );
        ReleaseDC(None, screen);
        copied.map_err(|e| format!("could not copy the screen: {e}"))?;
    }
    if cursor {
        draw_cursor(&canvas, (rect.x, rect.y));
    }
    Ok(canvas.into_image())
}

/// The whole virtual screen (every monitor, with the gaps between them black).
pub fn capture_virtual_screen(cursor: bool) -> Result<Image, String> {
    let bounds = desktop().bounds().ok_or("there is no monitor")?;
    capture_rect(bounds, cursor)
}

/// One monitor, by its position in [`desktop`] (0 is the primary).
pub fn capture_monitor(index: usize, cursor: bool) -> Result<Image, String> {
    let desktop = desktop();
    let monitor = desktop
        .monitors
        .get(index)
        .ok_or_else(|| format!("there is no monitor {index}"))?;
    capture_rect(monitor.bounds, cursor)
}

/// The window `handle`, whole even if other windows cover it, without the
/// invisible margin Windows adds around it.
pub fn capture_window(handle: isize, cursor: bool) -> Result<Image, String> {
    let hwnd = HWND(handle as *mut c_void);
    let mut outer = RECT::default();
    // SAFETY: plain queries on a window handle with valid out pointers.
    let outer = unsafe { GetWindowRect(hwnd, &mut outer) }
        .map(|()| outer)
        .map_err(|_| "that window is gone".to_string())?;
    let (w, h) = (
        (outer.right - outer.left).max(0) as u32,
        (outer.bottom - outer.top).max(0) as u32,
    );
    let canvas = Canvas::new(w, h)?;
    // SAFETY: the window draws itself into our DC.
    let drawn = unsafe { PrintWindow(hwnd, canvas.dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)) };
    if !drawn.as_bool() {
        return Err("that window does not let itself be captured".into());
    }
    if cursor {
        draw_cursor(&canvas, (outer.left, outer.top));
    }
    let image = canvas.into_image();

    // Windows 10 and 11 draw windows with an invisible border: cut to the
    // visible frame.
    let mut frame = RECT::default();
    // SAFETY: the attribute is a RECT, as the size passed says.
    let visible = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut frame as *mut RECT).cast::<c_void>(),
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .is_ok();
    if !visible {
        return Ok(image);
    }
    let inside = PxRect::new(
        frame.left - outer.left,
        frame.top - outer.top,
        (frame.right - frame.left).max(0) as u32,
        (frame.bottom - frame.top).max(0) as u32,
    );
    Ok(image.crop(&inside).unwrap_or(image))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session without a display (a service, some CI machines) has nothing to capture.
    fn has_screen() -> bool {
        !desktop().monitors.is_empty()
    }

    #[test]
    fn the_desktop_has_a_primary_monitor_first() {
        if !has_screen() {
            return;
        }
        let d = desktop();
        assert!(!d.monitors.is_empty());
        assert!(d.monitors[0].primary);
        assert!(d.monitors.iter().all(|m| m.scale_percent >= 100));
        assert!(d.bounds().is_some());
    }

    #[test]
    fn a_rectangle_of_the_screen_comes_out_at_the_size_asked() {
        if !has_screen() {
            return;
        }
        let d = desktop();
        let primary = d.monitors[0].bounds;
        let image = capture_rect(PxRect::new(primary.x, primary.y, 64, 48), false).unwrap();
        assert_eq!((image.width, image.height), (64, 48));
        assert!(image.is_valid());
        // Opaque, like a screen.
        assert!(image.bgra.as_chunks::<4>().0.iter().all(|px| px[3] == 255));
    }

    #[test]
    fn a_rectangle_sticking_out_is_cut_and_one_outside_fails() {
        if !has_screen() {
            return;
        }
        let d = desktop();
        let b = d.bounds().unwrap();
        let image = capture_rect(PxRect::new(b.right() - 10, b.y, 50, 20), false).unwrap();
        assert_eq!((image.width, image.height), (10, 20));
        assert!(capture_rect(PxRect::new(b.right() + 100, b.bottom() + 100, 5, 5), false).is_err());
    }

    #[test]
    fn the_whole_virtual_screen_has_the_size_of_its_bounds() {
        if !has_screen() {
            return;
        }
        let b = desktop().bounds().unwrap();
        let image = capture_virtual_screen(false).unwrap();
        assert_eq!((image.width, image.height), (b.w, b.h));
    }

    #[test]
    fn a_monitor_that_does_not_exist_and_a_window_that_is_gone_say_so() {
        assert!(capture_monitor(99, false).is_err());
        assert!(capture_window(0x1234_5678, false).is_err());
    }
}
