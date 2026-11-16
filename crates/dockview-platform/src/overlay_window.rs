//! The window of the selection overlay on Windows: a borderless, topmost
//! window over every monitor that only collects what the user does. It draws
//! nothing itself (the overlay's GPU does); mouse and keys come out of
//! [`OverlayWindow::pump`] as the portable [`Input`]s of the domain.

use std::ffi::c_void;

use dockview_domain::desktop::PxRect;
use dockview_domain::select::Input;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::ValidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, ReleaseCapture, SetCapture, VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowLongPtrW,
    LoadCursorW, PeekMessageW, RegisterClassW, SetCursor, SetForegroundWindow, SetWindowLongPtrW,
    ShowWindow, TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, IDC_ARROW, IDC_CROSS, MSG,
    PM_REMOVE, SW_SHOW, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_NCCREATE, WM_PAINT, WM_RBUTTONDOWN, WM_SETCURSOR, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

/// What the window procedure leaves for [`OverlayWindow::pump`].
struct State {
    events: Vec<Input>,
    origin: (i32, i32),
    arrow_cursor: bool,
}

pub struct OverlayWindow {
    hwnd: HWND,
    state: *mut State,
    size: (u32, u32),
    escape_was_down: bool,
}

impl OverlayWindow {
    /// A window over `bounds` (the virtual screen, in physical pixels).
    pub fn new(bounds: PxRect) -> Option<Self> {
        // SAFETY: a window of our own on this thread; the state pointer given to
        // it is freed in `Drop`, after the window is destroyed.
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let class = w!("DockviewCaptureOverlay");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            };
            // Registering twice fails harmlessly: the class is already there.
            RegisterClassW(&wc);
            let state = Box::into_raw(Box::new(State {
                events: Vec::new(),
                origin: (bounds.x, bounds.y),
                arrow_cursor: false,
            }));
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class,
                w!("Dockview capture"),
                WS_POPUP,
                bounds.x,
                bounds.y,
                bounds.w as i32,
                bounds.h as i32,
                None,
                None,
                Some(instance.into()),
                Some(state.cast::<c_void>()),
            );
            let Ok(hwnd) = hwnd else {
                drop(Box::from_raw(state));
                return None;
            };
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
            Some(Self {
                hwnd,
                state,
                size: (bounds.w, bounds.h),
                escape_was_down: false,
            })
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    /// The size of the window in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The cursor is an arrow (over buttons, or choosing a window) instead of a
    /// crosshair.
    pub fn set_arrow_cursor(&mut self, arrow: bool) {
        // SAFETY: `state` lives as long as `self`.
        unsafe { (*self.state).arrow_cursor = arrow };
    }

    /// Handles the window's messages and returns what the user did since the
    /// last call. Escape counts even if Windows did not give us the keyboard
    /// focus.
    pub fn pump(&mut self) -> Vec<Input> {
        // SAFETY: a normal message pump for this thread's windows.
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let mut events = std::mem::take(&mut (*self.state).events);
            let down = GetAsyncKeyState(i32::from(VK_ESCAPE.0)) as u16 & 0x8000 != 0;
            if down && !self.escape_was_down && !events.contains(&Input::Escape) {
                events.push(Input::Escape);
            }
            self.escape_was_down = down;
            events
        }
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        // SAFETY: the window goes first, then the state it pointed to.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            drop(Box::from_raw(self.state));
        }
    }
}

/// Where the mouse cursor is on the virtual screen.
pub fn cursor_position() -> Option<(i32, i32)> {
    let mut at = windows::Win32::Foundation::POINT::default();
    // SAFETY: a plain query with a valid out pointer.
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut at) }
        .ok()
        .map(|()| (at.x, at.y))
}

/// The pointer position of a mouse message, on the virtual screen.
fn point_of(lparam: LPARAM, origin: (i32, i32)) -> (i32, i32) {
    // The low and high words are signed 16-bit client coordinates.
    let x = i32::from((lparam.0 & 0xFFFF) as i16);
    let y = i32::from(((lparam.0 >> 16) & 0xFFFF) as i16);
    (x + origin.0, y + origin.1)
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: lparam is a CREATESTRUCTW for this message; its parameter is
        // the state pointer passed to CreateWindowExW.
        unsafe {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return LRESULT(1);
    }
    // SAFETY: the pointer was stored by WM_NCCREATE and stays valid until the
    // window is dropped.
    let state = unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).as_mut() };
    let Some(state) = state else {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    match message {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            // The GPU draws; there is nothing to paint here.
            // SAFETY: validating the whole window stops the paint messages.
            let _ = unsafe { ValidateRect(Some(hwnd), None) };
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            // SAFETY: keeps the mouse while the button is held.
            unsafe { SetCapture(hwnd) };
            let (x, y) = point_of(lparam, state.origin);
            state.events.push(Input::Down(x, y));
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = point_of(lparam, state.origin);
            state.events.push(Input::Move(x, y));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = point_of(lparam, state.origin);
            state.events.push(Input::Up(x, y));
            // SAFETY: releases the capture taken on the press.
            let _ = unsafe { ReleaseCapture() };
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            state.events.push(Input::Escape);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == usize::from(VK_ESCAPE.0) => {
            state.events.push(Input::Escape);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cursor = if state.arrow_cursor {
                IDC_ARROW
            } else {
                IDC_CROSS
            };
            // SAFETY: loads and sets a stock cursor.
            unsafe {
                if let Ok(c) = LoadCursorW(None, cursor) {
                    SetCursor(Some(c));
                }
            }
            LRESULT(1)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pointer_position_is_signed_and_moved_to_the_virtual_screen() {
        // x = 10, y = 20 packed as the low and high words.
        let at = |x: i16, y: i16| LPARAM(((y as u16 as isize) << 16) | (x as u16 as isize));
        assert_eq!(point_of(at(10, 20), (0, 0)), (10, 20));
        assert_eq!(point_of(at(10, 20), (-2560, -200)), (-2550, -180));
        assert_eq!(point_of(at(-5, -7), (100, 100)), (95, 93));
    }
}
