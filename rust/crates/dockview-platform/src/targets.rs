//! What can be captured: the monitors and the windows, as Windows lists them.

use std::ffi::c_void;

use dockview_domain::sources::{MonitorInfo, WindowInfo};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindow, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
};

unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<MonitorInfo>` passed to EnumDisplayMonitors.
    let list = unsafe { &mut *(data.0 as *mut Vec<MonitorInfo>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `info` is a valid MONITORINFOEXW with cbSize set.
    if unsafe {
        GetMonitorInfoW(
            monitor,
            (&mut info as *mut MONITORINFOEXW).cast::<MONITORINFO>(),
        )
    }
    .as_bool()
    {
        let r = info.monitorInfo.rcMonitor;
        let name_len = info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szDevice.len());
        list.push(MonitorInfo {
            handle: monitor.0 as isize,
            device_name: String::from_utf16_lossy(&info.szDevice[..name_len]),
            width: (r.right - r.left).max(0) as u32,
            height: (r.bottom - r.top).max(0) as u32,
            primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
        });
    }
    BOOL(1)
}

/// The monitors, the primary one first.
pub fn list_monitors() -> Vec<MonitorInfo> {
    let mut list: Vec<MonitorInfo> = Vec::new();
    // SAFETY: the callback only touches `list`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_callback),
            LPARAM(&mut list as *mut Vec<MonitorInfo> as isize),
        );
    }
    list.sort_by_key(|m| !m.primary);
    list
}

unsafe extern "system" fn window_callback(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<WindowInfo>` passed to EnumWindows.
    let list = unsafe { &mut *(data.0 as *mut Vec<WindowInfo>) };
    // SAFETY: plain queries on a window handle from EnumWindows.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return BOOL(1);
        }
        // Owned windows (dialogs) and tool windows are not "applications".
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null()) {
            return BOOL(1);
        }
        if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }
        // Windows on other virtual desktops and suspended apps are "cloaked".
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast::<c_void>(),
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return BOOL(1);
        }
        let mut buffer = [0u16; 512];
        let length = GetWindowTextW(hwnd, &mut buffer) as usize;
        let title = String::from_utf16_lossy(&buffer[..length]);
        if title.trim().is_empty() {
            return BOOL(1);
        }
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        list.push(WindowInfo {
            handle: hwnd.0 as isize,
            title,
            process_id,
        });
    }
    BOOL(1)
}

/// The application windows that can be captured, in the order Windows lists
/// them (front to back).
pub fn list_windows() -> Vec<WindowInfo> {
    let mut list: Vec<WindowInfo> = Vec::new();
    // SAFETY: the callback only touches `list`, which outlives the call.
    unsafe {
        let _ = EnumWindows(
            Some(window_callback),
            LPARAM(&mut list as *mut Vec<WindowInfo> as isize),
        );
    }
    list
}

/// Whether a window handle still refers to a window.
pub fn window_exists(handle: isize) -> bool {
    // SAFETY: IsWindow accepts any handle value.
    unsafe { IsWindow(Some(HWND(handle as *mut c_void))).as_bool() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitors_are_listed_with_the_primary_first() {
        let monitors = list_monitors();
        assert!(!monitors.is_empty(), "this PC has at least one monitor");
        assert!(monitors[0].primary);
        assert!(monitors.iter().all(|m| m.width > 0 && m.height > 0));
    }

    #[test]
    fn listing_windows_does_not_crash_and_skips_untitled_ones() {
        let windows = list_windows();
        assert!(windows
            .iter()
            .all(|w| !w.title.trim().is_empty() && w.process_id != 0));
    }

    #[test]
    fn a_made_up_window_handle_does_not_exist() {
        assert!(!window_exists(0x7FFF_FFF0));
    }
}
