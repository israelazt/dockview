//! The global hotkeys: "save the replay" (Ctrl+Alt+S), "start or stop
//! recording" (Ctrl+Alt+R) and the scene hotkeys (Ctrl+Alt+F1 to F9 for the
//! scenes, Ctrl+Alt+F12 for the plain source; function keys, because Ctrl+Alt is
//! AltGr on many layouts and would steal the characters typed with digits).
//!
//! Registered on a thread of its own with a message loop, so they work even
//! while another window has the focus and do not depend on winit.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, VK_F1, VK_F12, VK_R, VK_S,
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
/// The first of the ten scene hotkey ids (slot 0 is the plain source).
const SCENE_ID: i32 = 0x4B60;
/// Slots of the scene hotkeys: 0 (plain source) and 1 to 9 (scenes).
pub const SCENE_SLOTS: usize = 10;

/// The virtual key of a scene slot: F1 to F9 for the scenes, F12 for the plain
/// source.
fn scene_key(slot: usize) -> u32 {
    if slot == 0 {
        u32::from(VK_F12.0)
    } else {
        u32::from(VK_F1.0) + slot as u32 - 1
    }
}

#[derive(Default)]
struct Pressed {
    save: AtomicBool,
    record: AtomicBool,
    /// The scene slot pressed last, plus one (0: none).
    scene: AtomicUsize,
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

    /// The scene slot whose hotkey was pressed since the last call.
    pub fn take_scene_pressed(&self) -> Option<usize> {
        match self.pressed.scene.swap(0, Ordering::Relaxed) {
            0 => None,
            n => Some(n - 1),
        }
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
        // Another program may own some of the function keys: the rest still work.
        let scene_slots: Vec<usize> = (0..SCENE_SLOTS)
            .filter(|slot| {
                RegisterHotKey(None, SCENE_ID + *slot as i32, modifiers, scene_key(*slot)).is_ok()
            })
            .collect();
        let _ = ready.send(registered);
        if !registered.save && !registered.record && scene_slots.is_empty() {
            return;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            if message.message == WM_HOTKEY {
                match message.wParam.0 as i32 {
                    SAVE_ID => pressed.save.store(true, Ordering::Relaxed),
                    RECORD_ID => pressed.record.store(true, Ordering::Relaxed),
                    id if (SCENE_ID..SCENE_ID + SCENE_SLOTS as i32).contains(&id) => pressed
                        .scene
                        .store((id - SCENE_ID) as usize + 1, Ordering::Relaxed),
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
        for slot in scene_slots {
            let _ = UnregisterHotKey(None, SCENE_ID + slot as i32);
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
    fn the_scene_slots_map_to_f1_to_f9_and_f12() {
        assert_eq!(scene_key(1), u32::from(VK_F1.0));
        assert_eq!(scene_key(9), u32::from(VK_F1.0) + 8);
        assert_eq!(scene_key(0), u32::from(VK_F12.0));
        let ids: Vec<i32> = (0..SCENE_SLOTS as i32).map(|s| SCENE_ID + s).collect();
        assert!(!ids.contains(&SAVE_ID) && !ids.contains(&RECORD_ID));
    }

    #[test]
    fn registering_and_dropping_does_not_hang_or_leave_a_stale_press() {
        // Whether the combinations are free depends on the machine; either
        // outcome must be handled without hanging.
        let hotkey = Hotkey::register();
        assert!(!hotkey.take_pressed());
        assert!(!hotkey.take_record_pressed());
        assert_eq!(hotkey.take_scene_pressed(), None);
        drop(hotkey);
    }
}
