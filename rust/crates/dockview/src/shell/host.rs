//! The window as the application sees it: the real implementation of the
//! `Host` port.
//!
//! The application cannot hold the window (the shell owns it), so the two share
//! a small state: the shell writes what the application may ask about (the
//! window's size, the GPU time) and reads back what it asked to be done (full
//! screen, the video filter, a redraw).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use dockview_domain::ports::Host;
use dockview_domain::settings::RenderPreset;
use dockview_platform::clips;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
};

/// Something the application asked the window to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostRequest {
    Fullscreen(bool),
    Preset(RenderPreset),
    Redraw,
}

/// What the shell and the application share about the window.
#[derive(Debug, Default)]
pub struct HostShared {
    /// The window's handle, once it exists.
    pub hwnd: Option<HWND>,
    pub window_size: (u32, u32),
    pub monitor_hz: Option<f32>,
    pub gpu_ms: Option<f32>,
    /// What the application asked for, in order, until the shell does it.
    pub requests: Vec<HostRequest>,
}

/// The `Host` the application is given.
pub struct ShellHost {
    shared: Rc<RefCell<HostShared>>,
}

impl ShellHost {
    pub fn new(shared: Rc<RefCell<HostShared>>) -> Self {
        Self { shared }
    }

    fn ask(&self, request: HostRequest) {
        self.shared.borrow_mut().requests.push(request);
    }
}

impl Host for ShellHost {
    fn set_fullscreen(&self, on: bool) {
        self.ask(HostRequest::Fullscreen(on));
    }

    fn set_render_preset(&self, preset: RenderPreset) {
        self.ask(HostRequest::Preset(preset));
    }

    fn exclude_from_capture(&self, on: bool) {
        if let Some(hwnd) = self.shared.borrow().hwnd {
            let affinity = if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
            // SAFETY: a plain call on our own window; it fails harmlessly on
            // Windows versions that do not support it.
            unsafe {
                let _ = SetWindowDisplayAffinity(hwnd, affinity);
            }
        }
    }

    fn pick_folder(&self, start: &Path) -> Option<PathBuf> {
        let owner = self.shared.borrow().hwnd;
        clips::pick_folder(owner, start)
    }

    fn window_size(&self) -> (u32, u32) {
        self.shared.borrow().window_size
    }

    fn monitor_hz(&self) -> Option<f32> {
        self.shared.borrow().monitor_hz
    }

    fn gpu_ms(&self) -> Option<f32> {
        self.shared.borrow().gpu_ms
    }

    fn request_redraw(&self) {
        self.ask(HostRequest::Redraw);
    }
}
