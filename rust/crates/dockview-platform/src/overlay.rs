//! The selection overlay on Windows: a borderless window over every monitor
//! showing the frozen screen, dimmed, where the user drags, clicks or draws what
//! to capture. It implements `SelectionOverlay`; what the pointer does is
//! decided by the portable `Selector` of the domain, this only shows it and
//! feeds it events.
//!
//! Drawn with GDI into a back buffer: the dimmed picture everywhere, and the
//! chosen part at full brightness with a border and its size.

use std::ffi::c_void;

use dockview_domain::desktop::{Desktop, PxRect};
use dockview_domain::image::Image;
use dockview_domain::ports::SelectionOverlay;
use dockview_domain::select::{Input, Mode, Selection, Selector, Step, View};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, SetCapture, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, LoadCursorW, PostQuitMessage, RegisterClassW, SetCursor,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, ShowWindow, TranslateMessage, CREATESTRUCTW,
    GWLP_USERDATA, IDC_ARROW, IDC_CROSS, MSG, SW_SHOW, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_PAINT, WM_RBUTTONDOWN,
    WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::overlay_paint::{dimmed, paint};
use crate::snapshot::Canvas;

const TIMER_ID: usize = 1;
/// The real overlay.
pub struct PlatformOverlay;

/// Everything the window procedure needs, kept behind the window's user data.
pub(crate) struct State {
    pub(crate) selector: Selector,
    pub(crate) origin: (i32, i32),
    pub(crate) bright: Canvas,
    pub(crate) dim: Canvas,
    result: Option<Selection>,
    done: bool,
    /// The area that was drawn on last time, to repaint only what changed.
    last_dirty: Option<RECT>,
}

impl SelectionOverlay for PlatformOverlay {
    fn pick(
        &self,
        frozen: &Image,
        desktop: &Desktop,
        windows: Vec<PxRect>,
        mode: Mode,
    ) -> Option<Selection> {
        let bounds = desktop.bounds()?;
        let selector = Selector::new(mode, bounds, windows);
        // A mode that needs no pointer (the whole screen) is done already.
        match selector.start() {
            Step::Done(selection) => return Some(selection),
            Step::Cancel => return None,
            Step::Continue => {}
        }
        let state = Box::new(State {
            selector,
            origin: (bounds.x, bounds.y),
            bright: Canvas::from_image(frozen).ok()?,
            dim: Canvas::from_image(&dimmed(frozen)).ok()?,
            result: None,
            done: false,
            last_dirty: None,
        });
        // SAFETY: a window of our own on this thread; the state pointer given to
        // it is reclaimed below, after the window is gone.
        unsafe { run_window(state, bounds) }
    }
}

unsafe fn run_window(state: Box<State>, bounds: PxRect) -> Option<Selection> {
    let instance = unsafe { GetModuleHandleW(None) }.ok()?;
    let class = w!("DockviewSelectionOverlay");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        lpszClassName: class,
        ..Default::default()
    };
    // Registering twice fails harmlessly: the class is already there.
    unsafe { RegisterClassW(&wc) };
    let raw = Box::into_raw(state);
    let hwnd = unsafe {
        CreateWindowExW(
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
            Some(raw.cast::<c_void>()),
        )
    };
    let Ok(hwnd) = hwnd else {
        // SAFETY: the window was never made, so nothing else holds the state.
        drop(unsafe { Box::from_raw(raw) });
        return None;
    };
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        // Escape works even if Windows does not hand us the keyboard focus.
        SetTimer(Some(hwnd), TIMER_ID, 30, None);
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let _ = DestroyWindow(hwnd);
    }
    // SAFETY: the window procedure ends with the window; nothing uses `raw` now.
    let state = unsafe { Box::from_raw(raw) };
    state.result
}

fn state_of<'a>(hwnd: HWND) -> Option<&'a mut State> {
    // SAFETY: the pointer was stored by WM_NCCREATE and stays valid until
    // `run_window` takes the state back, after the message loop ended.
    let at = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut State;
    unsafe { at.as_mut() }
}

fn point_of(lparam: LPARAM, origin: (i32, i32)) -> (i32, i32) {
    // The low and high words are signed 16-bit client coordinates.
    let x = i32::from((lparam.0 & 0xFFFF) as i16);
    let y = i32::from(((lparam.0 >> 16) & 0xFFFF) as i16);
    (x + origin.0, y + origin.1)
}

/// The rectangle of the window to repaint for `view`, in client pixels.
fn dirty_of(view: &View, origin: (i32, i32)) -> Option<RECT> {
    let mut parts: Vec<PxRect> = Vec::new();
    parts.extend(view.rect);
    parts.extend(view.hover);
    if !view.path.is_empty() {
        parts.push(dockview_domain::select::bounding_rect(&view.path));
    }
    let all = parts.into_iter().reduce(|a, b| a.union(&b))?;
    let pad = 40;
    Some(RECT {
        left: all.x - origin.0 - pad,
        top: all.y - origin.1 - pad,
        right: all.right() - origin.0 + pad,
        bottom: all.bottom() - origin.1 + pad,
    })
}

fn union(a: RECT, b: RECT) -> RECT {
    RECT {
        left: a.left.min(b.left),
        top: a.top.min(b.top),
        right: a.right.max(b.right),
        bottom: a.bottom.max(b.bottom),
    }
}

/// Feeds the selector and repaints what changed.
fn feed(hwnd: HWND, state: &mut State, input: Input) {
    let before = dirty_of(&state.selector.view(), state.origin);
    match state.selector.apply(input) {
        Step::Continue => {}
        Step::Done(selection) => {
            state.result = Some(selection);
            state.done = true;
        }
        Step::Cancel => state.done = true,
    }
    if state.done {
        // SAFETY: ends this thread's message loop.
        unsafe { PostQuitMessage(0) };
        return;
    }
    let after = dirty_of(&state.selector.view(), state.origin);
    let area = match (before, after) {
        (Some(a), Some(b)) => Some(union(a, b)),
        (a, b) => a.or(b),
    };
    state.last_dirty = after;
    // SAFETY: invalidating a rectangle of our own window.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), area.as_ref().map(|r| r as *const RECT), false);
    }
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
    let Some(state) = state_of(hwnd) else {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    match message {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint(hwnd, state);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            // SAFETY: keeps the mouse while the button is held.
            unsafe { SetCapture(hwnd) };
            let (x, y) = point_of(lparam, state.origin);
            feed(hwnd, state, Input::Down(x, y));
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = point_of(lparam, state.origin);
            feed(hwnd, state, Input::Move(x, y));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = point_of(lparam, state.origin);
            feed(hwnd, state, Input::Up(x, y));
            // SAFETY: releases the capture taken on the press.
            let _ = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture() };
            LRESULT(0)
        }
        WM_RBUTTONDOWN => {
            feed(hwnd, state, Input::Escape);
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == usize::from(VK_ESCAPE.0) => {
            feed(hwnd, state, Input::Escape);
            LRESULT(0)
        }
        WM_TIMER => {
            // SAFETY: a plain key-state query.
            if unsafe { GetAsyncKeyState(i32::from(VK_ESCAPE.0)) } as u16 & 0x8000 != 0 {
                feed(hwnd, state, Input::Escape);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cursor = if state.selector.mode() == Mode::Window {
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
        WM_DESTROY => LRESULT(0),
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

    #[test]
    fn the_area_to_repaint_covers_what_is_drawn_with_a_margin() {
        let view = View {
            rect: Some(PxRect::new(100, 100, 50, 40)),
            ..View::default()
        };
        let d = dirty_of(&view, (0, 0)).unwrap();
        assert!(d.left < 100 && d.top < 100 && d.right > 150 && d.bottom > 140);
        assert!(dirty_of(&View::default(), (0, 0)).is_none());
        let moved = dirty_of(&view, (50, 50)).unwrap();
        assert_eq!(moved.left, d.left - 50);
    }

    #[test]
    fn the_whole_screen_needs_no_window_and_no_screen_means_none() {
        let desktop = Desktop::default();
        let image = Image::solid(2, 2, [0, 0, 0, 255]);
        assert!(PlatformOverlay
            .pick(&image, &desktop, vec![], Mode::Full)
            .is_none());
    }
}
