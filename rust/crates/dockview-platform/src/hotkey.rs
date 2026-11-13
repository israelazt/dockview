//! The global hotkeys: "save the replay" (Ctrl+Alt+S) and "start or stop
//! recording" (Ctrl+Alt+R).
//!
//! Registered on a thread of its own with a message loop, so they work even
//! while another window has the focus and do not depend on winit.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_R, VK_S,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT,
};

/// What the user presses to save the replay, as shown in the interface.
pub const LABEL: &str = "Ctrl+Alt+S";
/// What the user presses to start or stop recording.
pub const RECORD_LABEL: &str = "Ctrl+Alt+R";
const SAVE_ID: i32 = 0x4B50;
const RECORD_ID: i32 = 0x4B51;

#[derive(Default)]
struct Pressed {
    save: AtomicBool,
    record: AtomicBool,
}

/// Which hotkeys could be registered (another program may own a combination).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Registered {
    save: bool,
    record: bool,
}

pub struct Hotkey {
    pressed: Arc<Pressed>,
    thread_id: Arc<AtomicU32>,
    thread: Option<JoinHandle<()>>,
    registered: Registered,
}

impl Hotkey {
    /// Registers the hotkeys. Check [`Hotkey::is_registered`] and
    /// [`Hotkey::record_registered`]: another program may already own a
    /// combination.
    pub fn register() -> Self {
        let pressed = Arc::new(Pressed::default());
        let thread_id = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::channel();

        let thread = {
            let (pressed, thread_id) = (Arc::clone(&pressed), Arc::clone(&thread_id));
            thread::Builder::new()
                .name("global-hotkey".into())
                .spawn(move || run(&pressed, &thread_id, &tx))
                .ok()
        };
        let registered = if thread.is_some() {
            rx.recv_timeout(Duration::from_secs(2)).unwrap_or_default()
        } else {
            Registered::default()
        };
        Self {
            pressed,
            thread_id,
            thread,
            registered,
        }
    }

    /// Whether the "save the replay" hotkey is registered.
    pub fn is_registered(&self) -> bool {
        self.registered.save
    }

    /// Whether the "record" hotkey is registered.
    pub fn record_registered(&self) -> bool {
        self.registered.record
    }

    /// Whether "save the replay" was pressed since the last call.
    pub fn take_pressed(&self) -> bool {
        self.pressed.save.swap(false, Ordering::Relaxed)
    }

    /// Whether "record" was pressed since the last call.
    pub fn take_record_pressed(&self) -> bool {
        self.pressed.record.swap(false, Ordering::Relaxed)
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

fn run(pressed: &Pressed, thread_id: &AtomicU32, ready: &mpsc::Sender<Registered>) {
    // SAFETY: registrations and a message loop on this thread only; the
    // hotkeys are unregistered before the thread ends.
    unsafe {
        thread_id.store(GetCurrentThreadId(), Ordering::Relaxed);
        let modifiers = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
        let registered = Registered {
            save: RegisterHotKey(None, SAVE_ID, modifiers, u32::from(VK_S.0)).is_ok(),
            record: RegisterHotKey(None, RECORD_ID, modifiers, u32::from(VK_R.0)).is_ok(),
        };
        let _ = ready.send(registered);
        if !registered.save && !registered.record {
            return;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            if message.message == WM_HOTKEY {
                match message.wParam.0 as i32 {
                    SAVE_ID => pressed.save.store(true, Ordering::Relaxed),
                    RECORD_ID => pressed.record.store(true, Ordering::Relaxed),
                    _ => {}
                }
            }
        }
        if registered.save {
            let _ = UnregisterHotKey(None, SAVE_ID);
        }
        if registered.record {
            let _ = UnregisterHotKey(None, RECORD_ID);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_labels_name_the_combinations() {
        assert_eq!(LABEL, "Ctrl+Alt+S");
        assert_eq!(RECORD_LABEL, "Ctrl+Alt+R");
        assert_ne!(SAVE_ID, RECORD_ID);
    }

    #[test]
    fn registering_and_dropping_does_not_hang_or_leave_a_stale_press() {
        // Whether the combinations are free depends on the machine; either
        // outcome must be handled without hanging.
        let hotkey = Hotkey::register();
        assert!(!hotkey.take_pressed());
        assert!(!hotkey.take_record_pressed());
        drop(hotkey);
    }
}
