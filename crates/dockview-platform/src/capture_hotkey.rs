//! The global hotkey that starts a screenshot (Print Screen by default, or what
//! the user chose), on Windows. Like the replay hotkeys it lives on a thread of
//! its own with a message loop, so it works while another window has the focus
//! and does not depend on winit. Dropping it gives the combination back.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dockview_domain::hotkey::{HotkeySpec, Key};
use dockview_domain::ports::{CaptureHotkeys, CaptureTrigger};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PostThreadMessageW, MSG, WM_HOTKEY, WM_QUIT,
};

/// The id of the hotkey on its thread.
const HOTKEY_ID: i32 = 0x4B70;

/// The Windows virtual-key code of a key.
pub fn virtual_key(key: Key) -> u32 {
    match key {
        Key::Letter(c) => u32::from(c.to_ascii_uppercase() as u8),
        Key::Digit(d) => 0x30 + u32::from(d),
        Key::Function(n) => 0x70 + u32::from(n) - 1,
        Key::PrintScreen => 0x2C,
        Key::Pause => 0x13,
        Key::ScrollLock => 0x91,
        Key::Insert => 0x2D,
        Key::Delete => 0x2E,
        Key::Home => 0x24,
        Key::End => 0x23,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
    }
}

/// The modifiers of a combination, as `RegisterHotKey` takes them.
fn modifiers(spec: &HotkeySpec) -> HOT_KEY_MODIFIERS {
    let mut m = MOD_NOREPEAT;
    if spec.ctrl {
        m |= MOD_CONTROL;
    }
    if spec.alt {
        m |= MOD_ALT;
    }
    if spec.shift {
        m |= MOD_SHIFT;
    }
    if spec.win {
        m |= MOD_WIN;
    }
    m
}

/// Whether Windows keeps Print Screen for its own Snipping Tool: the setting
/// `Settings > Accessibility > Keyboard > Use the Print screen button to open
/// screen capture` (on by default in Windows 11). While it is on, Windows lets a
/// program register Print Screen but never delivers the key to it, so Dockview
/// has to ask the user to turn it off.
pub fn windows_owns_print_screen() -> bool {
    owns_print_screen(read_snipping_setting())
}

/// The rule: the setting is on unless it says 0 (a Windows that never wrote it
/// uses the default, on).
fn owns_print_screen(value: Option<u32>) -> bool {
    value != Some(0)
}

fn read_snipping_setting() -> Option<u32> {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: reads a DWORD into `data`, which has room for it.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Control Panel\Keyboard"),
            w!("PrintScreenKeyForSnippingEnabled"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(data)
}

/// Opens the Windows page where that setting is.
pub fn open_keyboard_settings() {
    use windows::core::w;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    // SAFETY: asks the shell to open a settings URI.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("ms-settings:easeofaccess-keyboard"),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

/// A registered hotkey.
pub struct CaptureHotkey {
    pressed: Arc<AtomicBool>,
    thread_id: Arc<AtomicU32>,
    thread: Option<JoinHandle<()>>,
}

impl CaptureHotkey {
    /// Registers `spec`; the error says why not (another program has it).
    pub fn register(spec: HotkeySpec) -> Result<Self, String> {
        let pressed = Arc::new(AtomicBool::new(false));
        let thread_id = Arc::new(AtomicU32::new(0));
        let (tx, rx) = mpsc::channel();
        let thread = {
            let (pressed, thread_id) = (Arc::clone(&pressed), Arc::clone(&thread_id));
            thread::Builder::new()
                .name("capture-hotkey".into())
                .spawn(move || run(spec, &pressed, &thread_id, &tx))
                .map_err(|e| format!("could not start the hotkey thread: {e}"))?
        };
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(Self {
                pressed,
                thread_id,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("the hotkey thread did not answer".into()),
        }
    }
}

impl CaptureTrigger for CaptureHotkey {
    fn take_pressed(&self) -> bool {
        self.pressed.swap(false, Ordering::Relaxed)
    }
}

impl Drop for CaptureHotkey {
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

fn run(
    spec: HotkeySpec,
    pressed: &AtomicBool,
    thread_id: &AtomicU32,
    ready: &mpsc::Sender<Result<(), String>>,
) {
    // SAFETY: the registration and a message loop on this thread only; the
    // hotkey is unregistered before the thread ends.
    unsafe {
        thread_id.store(GetCurrentThreadId(), Ordering::Relaxed);
        if let Err(e) = RegisterHotKey(None, HOTKEY_ID, modifiers(&spec), virtual_key(spec.key)) {
            let _ = ready.send(Err(format!(
                "{spec} is already used by Windows or another program ({e})"
            )));
            return;
        }
        let _ = ready.send(Ok(()));
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            if message.message == WM_HOTKEY && message.wParam.0 as i32 == HOTKEY_ID {
                pressed.store(true, Ordering::Relaxed);
            }
        }
        let _ = UnregisterHotKey(None, HOTKEY_ID);
    }
}

/// The real hotkeys of this PC.
pub struct PlatformCaptureHotkeys;

impl CaptureHotkeys for PlatformCaptureHotkeys {
    fn register(&self, spec: &HotkeySpec) -> Result<Box<dyn CaptureTrigger>, String> {
        CaptureHotkey::register(*spec).map(|h| Box::new(h) as Box<dyn CaptureTrigger>)
    }

    fn kept_by_system(&self, spec: &HotkeySpec) -> bool {
        *spec == HotkeySpec::DEFAULT_CAPTURE && windows_owns_print_screen()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A combination nobody uses: Ctrl+Shift+Alt+F20.
    fn rare() -> HotkeySpec {
        HotkeySpec::parse("Ctrl+Shift+Alt+F20").unwrap()
    }

    #[test]
    fn keys_map_to_the_windows_codes() {
        assert_eq!(virtual_key(Key::PrintScreen), 0x2C);
        assert_eq!(virtual_key(Key::Letter('s')), 0x53);
        assert_eq!(virtual_key(Key::Letter('Z')), 0x5A);
        assert_eq!(virtual_key(Key::Digit(0)), 0x30);
        assert_eq!(virtual_key(Key::Digit(9)), 0x39);
        assert_eq!(virtual_key(Key::Function(1)), 0x70);
        assert_eq!(virtual_key(Key::Function(12)), 0x7B);
        assert_eq!(virtual_key(Key::Function(24)), 0x87);
        assert_eq!(virtual_key(Key::PageDown), 0x22);
    }

    #[test]
    fn the_modifiers_follow_the_spec_and_never_repeat() {
        let m = modifiers(&HotkeySpec::parse("Ctrl+Win+F9").unwrap());
        assert_eq!(m.0 & MOD_CONTROL.0, MOD_CONTROL.0);
        assert_eq!(m.0 & MOD_WIN.0, MOD_WIN.0);
        assert_eq!(m.0 & MOD_ALT.0, 0);
        assert_eq!(m.0 & MOD_NOREPEAT.0, MOD_NOREPEAT.0);
    }

    #[test]
    fn a_combination_somebody_has_cannot_be_registered_again_and_is_free_after_a_drop() {
        let first = CaptureHotkey::register(rare());
        let Ok(first) = first else {
            return; // this PC already uses it: nothing to prove here
        };
        let second = CaptureHotkey::register(rare());
        let Err(why) = second else {
            panic!("the same combination was registered twice");
        };
        assert!(why.contains("already used"), "{why}");
        drop(first);
        let again = CaptureHotkey::register(rare());
        assert!(again.is_ok(), "the combination was not given back");
    }

    #[test]
    fn print_screen_is_windows_unless_the_setting_says_off() {
        assert!(owns_print_screen(Some(1)));
        assert!(owns_print_screen(None), "never written: the default is on");
        assert!(!owns_print_screen(Some(0)));
    }

    #[test]
    fn reading_the_setting_does_not_panic_whatever_it_is_here() {
        let _ = windows_owns_print_screen();
    }

    #[test]
    fn nothing_is_pressed_until_the_key_is() {
        if let Ok(hotkey) = CaptureHotkey::register(rare()) {
            assert!(!hotkey.take_pressed());
        }
    }

    /// Whether Windows lets this PC register Print Screen, and whether pressing
    /// it (sent from here) is seen. Presses keys on the machine, so it only
    /// runs on purpose:
    /// `cargo test -p dockview-platform -- --ignored print_screen --nocapture`.
    #[test]
    #[ignore]
    fn print_screen_can_be_registered_and_is_seen_here() {
        use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};
        let spec = HotkeySpec::DEFAULT_CAPTURE;
        let hotkey = match CaptureHotkey::register(spec) {
            Ok(h) => h,
            Err(why) => {
                println!("PrintScreen NOT registered: {why}");
                return;
            }
        };
        println!("PrintScreen registered");
        if windows_owns_print_screen() {
            // Pressing it now would open Windows' Snipping Tool on the screen.
            println!("Windows keeps Print Screen for its Snipping Tool: not pressed");
            return;
        }
        // SAFETY: injects a key press and release of Print Screen.
        unsafe {
            keybd_event(0x2C, 0x37, Default::default(), 0);
            std::thread::sleep(Duration::from_millis(50));
            keybd_event(0x2C, 0x37, KEYEVENTF_KEYUP, 0);
        }
        std::thread::sleep(Duration::from_millis(400));
        println!("PrintScreen seen by the hotkey: {}", hotkey.take_pressed());
    }
}
