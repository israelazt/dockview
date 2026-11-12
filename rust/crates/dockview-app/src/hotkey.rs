//! The global "save the replay" hotkey.
//!
//! Registered on a thread of its own with a message loop, so it works even
//! while another window has the focus and does not depend on winit.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_S,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT,
};

/// What the user presses, as shown in the interface.
pub const LABEL: &str = "Ctrl+Alt+S";
const HOTKEY_ID: i32 = 0x4B50;

pub struct Hotkey {
    pressed: Arc<AtomicBool>,
    thread_id: Arc<AtomicU32>,
    thread: Option<JoinHandle<()>>,
    registered: bool,
}

impl Hotkey {
    /// Registers the hotkey. Check [`Hotkey::is_registered`]: another program
    /// may already own the combination.
    pub fn register() -> Self {
        let pressed = Arc::new(AtomicBool::new(false));
        let thread_id = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::channel();

        let thread = {
            let (pressed, thread_id) = (Arc::clone(&pressed), Arc::clone(&thread_id));
            thread::Builder::new()
                .name("global-hotkey".into())
                .spawn(move || run(&pressed, &thread_id, &tx))
                .ok()
        };
        let registered =
            thread.is_some() && rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false);
        Self {
            pressed,
            thread_id,
            thread,
            registered,
        }
    }

    pub fn is_registered(&self) -> bool {
        self.registered
    }

    /// Whether the hotkey was pressed since the last call.
    pub fn take_pressed(&self) -> bool {
        self.pressed.swap(false, Ordering::Relaxed)
    }
}

impl Drop for Hotkey {
    fn drop(&mut self) {
        let id = self.thread_id.load(Ordering::Relaxed);
        if id != 0 {
            // SAFETY: posting WM_QUIT to our own helper thread ends its loop.
            unsafe {
                let _ = PostThreadMessageW(id, WM_QUIT, Default::default(), Default::default());
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(pressed: &AtomicBool, thread_id: &AtomicU32, ready: &mpsc::Sender<bool>) {
    // SAFETY: a registration and a message loop on this thread only; the
    // hotkey is unregistered before the thread ends.
    unsafe {
        thread_id.store(GetCurrentThreadId(), Ordering::Relaxed);
        let ok = RegisterHotKey(
            None,
            HOTKEY_ID,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            u32::from(VK_S.0),
        )
        .is_ok();
        let _ = ready.send(ok);
        if !ok {
            return;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            if message.message == WM_HOTKEY && message.wParam.0 as i32 == HOTKEY_ID {
                pressed.store(true, Ordering::Relaxed);
            }
        }
        let _ = UnregisterHotKey(None, HOTKEY_ID);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_names_the_combination() {
        assert_eq!(LABEL, "Ctrl+Alt+S");
    }

    #[test]
    fn registering_and_dropping_does_not_hang_or_leave_a_stale_press() {
        // Whether the combination is free depends on the machine; either
        // outcome must be handled without hanging.
        let hotkey = Hotkey::register();
        assert!(!hotkey.take_pressed());
        drop(hotkey);
    }
}
