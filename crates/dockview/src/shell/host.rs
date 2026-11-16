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

use crate::os::{self, NativeWindow};
use dockview_domain::ports::Host;
use dockview_domain::settings::RenderPreset;

/// Something the application asked the window to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HostRequest {
    Fullscreen(bool),
    Preset(RenderPreset),
    Redraw,
    /// Start a screenshot in this mode.
    Capture(dockview_domain::select::Mode),
    /// Show the window and bring it forward.
    ShowWindow,
    /// End the program.
    Quit,
}

/// What the shell and the application share about the window.
#[derive(Debug, Default)]
pub struct HostShared {
    /// The window's native handle, once it exists.
    pub native: Option<NativeWindow>,
    pub window_size: (u32, u32),
    /// The monitor the window is on, by the system's name.
    pub monitor_name: Option<String>,
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
        if let Some(window) = self.shared.borrow().native {
            os::exclude_from_capture(window, on);
        }
    }

    fn pick_folder(&self, start: &Path) -> Option<PathBuf> {
        let owner = self.shared.borrow().native;
        os::pick_folder(owner, start)
    }

    fn window_size(&self) -> (u32, u32) {
        self.shared.borrow().window_size
    }

    fn window_monitor(&self) -> Option<String> {
        self.shared.borrow().monitor_name.clone()
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

    fn start_capture(&self, mode: dockview_domain::select::Mode) {
        self.ask(HostRequest::Capture(mode));
    }

    fn show_window(&self) {
        self.ask(HostRequest::ShowWindow);
    }

    fn reveal_file(&self, path: &Path) {
        os::reveal_file(path);
    }

    fn quit(&self) {
        self.ask(HostRequest::Quit);
    }
}
