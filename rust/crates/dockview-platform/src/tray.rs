//! The tray icon on Windows: an icon in the notification area with a menu, run
//! by a hidden window on a thread of its own (so it works while Dockview's
//! window is closed). A click on the icon opens Dockview; the right button shows
//! the menu. Implements the `TrayHost` port.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dockview_domain::tray::{MenuEntry, Tray, TrayEvent, TrayHost, TrayLabels};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetWindowLongPtrW, LoadIconW, PostMessageW,
    RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, SetWindowLongPtrW, TrackPopupMenu,
    TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, HICON, IDI_APPLICATION, MF_DISABLED, MF_GRAYED,
    MF_SEPARATOR, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
    WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_LBUTTONUP, WM_NCCREATE, WM_NULL, WM_RBUTTONUP,
    WNDCLASSW, WS_POPUP,
};

/// The message the shell sends us for what happens on the icon.
const WM_TRAY: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;
/// The id of the icon resource `build.rs` embeds in `dockview.exe`.
const APP_ICON_RESOURCE: usize = 1;

/// What the window procedure needs.
struct State {
    labels: TrayLabels,
    events: Arc<Mutex<Vec<TrayEvent>>>,
    /// The message Windows broadcasts when the taskbar is created again
    /// (Explorer restarted): the icon is lost and must be added again.
    taskbar_created: u32,
}

pub struct PlatformTray {
    events: Arc<Mutex<Vec<TrayEvent>>>,
    hwnd: Arc<AtomicIsize>,
    thread: Option<JoinHandle<()>>,
}

impl Tray for PlatformTray {
    fn poll(&self) -> Vec<TrayEvent> {
        std::mem::take(&mut *self.events.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl Drop for PlatformTray {
    fn drop(&mut self) {
        let hwnd = self.hwnd.load(Ordering::Relaxed);
        if hwnd != 0 {
            // SAFETY: asks the tray's own window to close, which ends its thread.
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The real tray.
pub struct PlatformTrayHost;

impl TrayHost for PlatformTrayHost {
    fn show(&self, labels: &TrayLabels) -> Result<Box<dyn Tray>, String> {
        let events = Arc::new(Mutex::new(Vec::new()));
        let hwnd = Arc::new(AtomicIsize::new(0));
        let (tx, rx) = mpsc::channel();
        let thread = {
            let (labels, events, hwnd) = (labels.clone(), Arc::clone(&events), Arc::clone(&hwnd));
            thread::Builder::new()
                .name("tray".into())
                .spawn(move || run(labels, events, &hwnd, &tx))
                .map_err(|e| format!("could not start the tray thread: {e}"))?
        };
        match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(())) => Ok(Box::new(PlatformTray {
                events,
                hwnd,
                thread: Some(thread),
            })),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("the tray thread did not answer".into()),
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// The icon of the program (the one in the `.exe`), or a plain one when there
/// is none (the tests).
fn app_icon() -> HICON {
    // SAFETY: loads icons by resource id; a missing one falls back to the stock icon.
    unsafe {
        let instance = GetModuleHandleW(None).ok();
        instance
            .and_then(|i| LoadIconW(Some(i.into()), PCWSTR(APP_ICON_RESOURCE as *const u16)).ok())
            .or_else(|| LoadIconW(None, IDI_APPLICATION).ok())
            .unwrap_or_default()
    }
}

fn notify_data(hwnd: HWND, tooltip: &str) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: app_icon(),
        ..Default::default()
    };
    for (slot, unit) in data.szTip.iter_mut().zip(tooltip.encode_utf16().take(127)) {
        *slot = unit;
    }
    data
}

fn run(
    labels: TrayLabels,
    events: Arc<Mutex<Vec<TrayEvent>>>,
    hwnd_out: &AtomicIsize,
    ready: &mpsc::Sender<Result<(), String>>,
) {
    // SAFETY: a window of our own on this thread, which also pumps its messages.
    unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            let _ = ready.send(Err("no module handle".into()));
            return;
        };
        let class = w!("DockviewTrayWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let state = Box::into_raw(Box::new(State {
            labels,
            events,
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
        }));
        let hwnd = CreateWindowExW(
            Default::default(),
            class,
            w!("Dockview tray"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            Some(state.cast::<c_void>()),
        );
        let Ok(hwnd) = hwnd else {
            drop(Box::from_raw(state));
            let _ = ready.send(Err("could not create the tray window".into()));
            return;
        };
        let data = notify_data(hwnd, &(*state).labels.tooltip);
        if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
            let _ = DestroyWindow(hwnd);
            drop(Box::from_raw(state));
            let _ = ready.send(Err("Windows did not accept the tray icon".into()));
            return;
        }
        hwnd_out.store(hwnd.0 as isize, Ordering::Relaxed);
        let _ = ready.send(Ok(()));
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        drop(Box::from_raw(state));
    }
}

/// The menu built from the labels; item ids are positions + 1 in `items`.
unsafe fn show_menu(hwnd: HWND, labels: &TrayLabels) -> Option<TrayEvent> {
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    let mut items: Vec<TrayEvent> = Vec::new();
    for entry in &labels.menu {
        match entry {
            MenuEntry::Heading(text) => {
                let text = wide(text);
                let _ = unsafe {
                    AppendMenuW(
                        menu,
                        MF_STRING | MF_DISABLED | MF_GRAYED,
                        0,
                        PCWSTR(text.as_ptr()),
                    )
                };
            }
            MenuEntry::Item(text, event) => {
                items.push(*event);
                let text = wide(text);
                let _ = unsafe { AppendMenuW(menu, MF_STRING, items.len(), PCWSTR(text.as_ptr())) };
            }
            MenuEntry::Separator => {
                let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) };
            }
        }
    }
    let mut at = POINT::default();
    let _ = unsafe { GetCursorPos(&mut at) };
    // The menu closes when the user clicks elsewhere only if its window has the
    // foreground (a classic quirk of tray menus).
    let _ = unsafe { SetForegroundWindow(hwnd) };
    let chosen = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            at.x,
            at.y,
            None,
            hwnd,
            None,
        )
    };
    let _ = unsafe { PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0)) };
    let _ = unsafe { DestroyMenu(menu) };
    let id = chosen.0 as usize;
    (id >= 1).then(|| items.get(id - 1).copied()).flatten()
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: lparam is a CREATESTRUCTW; its parameter is the state pointer.
        unsafe {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return LRESULT(1);
    }
    // SAFETY: stored by WM_NCCREATE; alive until the message loop ends.
    let state = unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State).as_ref() };
    let Some(state) = state else {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    let push = |event: TrayEvent| {
        state
            .events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(event);
    };
    match message {
        WM_TRAY => {
            // The low word of lparam is what the mouse did on the icon.
            match (lparam.0 & 0xFFFF) as u32 {
                WM_LBUTTONUP => push(TrayEvent::Open),
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    // SAFETY: shows a menu on this thread, which owns the window.
                    if let Some(event) = unsafe { show_menu(hwnd, &state.labels) } {
                        push(event);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // SAFETY: takes our icon out of the tray before the window goes.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &notify_data(hwnd, ""));
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0) };
            LRESULT(0)
        }
        m if m == state.taskbar_created => {
            // Explorer started again: the icon has to be put back.
            // SAFETY: adds our icon to the new taskbar.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_ADD, &notify_data(hwnd, &state.labels.tooltip));
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dockview_domain::i18n::Lang;

    #[test]
    fn the_icon_comes_up_reports_nothing_and_goes_away_cleanly() {
        let host = PlatformTrayHost;
        let Ok(tray) = host.show(&TrayLabels::new(Lang::En)) else {
            return; // a session with no taskbar (a service, some CI machines)
        };
        assert!(tray.poll().is_empty());
        drop(tray);
        // And it can come back after it went.
        if let Ok(again) = host.show(&TrayLabels::new(Lang::Es)) {
            assert!(again.poll().is_empty());
        }
    }

    #[test]
    fn text_is_made_wide_and_ends_with_a_zero() {
        assert_eq!(wide("Ab"), [65, 98, 0]);
        assert_eq!(wide(""), [0]);
    }
}
