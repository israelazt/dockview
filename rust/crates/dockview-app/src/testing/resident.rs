//! Fakes of the capture hotkey and the tray: they note what was asked and let a
//! test press the hotkey or click the icon.

use std::sync::{Arc, Mutex};

use dockview_domain::hotkey::HotkeySpec;
use dockview_domain::ports::{CaptureHotkeys, CaptureTrigger};
use dockview_domain::tray::{Tray, TrayEvent, TrayHost, TrayLabels};

#[derive(Default)]
pub struct ResidentWorld {
    /// Every hotkey that was registered, in order.
    pub registered: Vec<HotkeySpec>,
    /// Hotkeys that were given back (their trigger was dropped).
    pub released: Vec<HotkeySpec>,
    /// Combinations another program has: registering them fails.
    pub busy: Vec<HotkeySpec>,
    /// Combinations the system keeps for itself.
    pub kept: Vec<HotkeySpec>,
    /// The hotkey was pressed.
    pub pressed: bool,
    /// Every tray icon that was shown, with its words.
    pub tray_shown: Vec<TrayLabels>,
    /// Icons in the tray now.
    pub tray_alive: u32,
    /// What the user did on the icon, until it is read.
    pub tray_events: Vec<TrayEvent>,
    /// Make showing the icon fail.
    pub tray_fails: bool,
}

pub type Shared = Arc<Mutex<ResidentWorld>>;

pub struct FakeHotkeys(pub Shared);
pub struct FakeTrayHost(pub Shared);
struct FakeTrigger(Shared, HotkeySpec);
struct FakeTray(Shared);

impl CaptureHotkeys for FakeHotkeys {
    fn register(&self, spec: &HotkeySpec) -> Result<Box<dyn CaptureTrigger>, String> {
        let mut world = self.0.lock().unwrap();
        if world.busy.contains(spec) {
            return Err("another program has it".into());
        }
        world.registered.push(*spec);
        Ok(Box::new(FakeTrigger(Arc::clone(&self.0), *spec)))
    }

    fn kept_by_system(&self, spec: &HotkeySpec) -> bool {
        self.0.lock().unwrap().kept.contains(spec)
    }
}

impl CaptureTrigger for FakeTrigger {
    fn take_pressed(&self) -> bool {
        std::mem::take(&mut self.0.lock().unwrap().pressed)
    }
}

impl Drop for FakeTrigger {
    fn drop(&mut self) {
        self.0.lock().unwrap().released.push(self.1);
    }
}

impl TrayHost for FakeTrayHost {
    fn show(&self, labels: &TrayLabels) -> Result<Box<dyn Tray>, String> {
        let mut world = self.0.lock().unwrap();
        if world.tray_fails {
            return Err("no taskbar".into());
        }
        world.tray_shown.push(labels.clone());
        world.tray_alive += 1;
        Ok(Box::new(FakeTray(Arc::clone(&self.0))))
    }
}

impl Tray for FakeTray {
    fn poll(&self) -> Vec<TrayEvent> {
        std::mem::take(&mut self.0.lock().unwrap().tray_events)
    }
}

impl Drop for FakeTray {
    fn drop(&mut self) {
        self.0.lock().unwrap().tray_alive -= 1;
    }
}
