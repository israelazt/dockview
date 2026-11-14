//! The window and everything that lives in it: the video renderer, the
//! interface, the pointer and the cursor. It feeds the application core what
//! happens (frames, keys, clicks, ticks) and draws what the core shows.

mod events;
mod keys;
mod studio;
mod window;

pub mod helpers;
pub mod host;
pub mod present;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dockview_app::Core;
use dockview_domain::i18n;
use dockview_platform::render::Renderer;
use dockview_platform::ui_gpu::UiGpu;
use dockview_ui::controls::Controls;
use dockview_ui::input::Pointer;
use dockview_ui::stats::StatsPanel;
use dockview_ui::studio::{StudioPanel, StudioSection};
use dockview_ui::toast::{ToastHost, ToastKind};
use dockview_ui::Ui;
use winit::window::Window;

use helpers::{filter_for, system_animations_enabled};
use host::{HostRequest, HostShared};
use present::{report_startup_failure, toast_for, StartupFailure};

/// How long the event loop waits for a new frame before checking events.
const FRAME_WAIT: Duration = Duration::from_millis(4);
/// How long to wait before trying to rebuild a renderer that could not be built.
const GRAPHICS_RETRY: Duration = Duration::from_secs(1);
/// Frames to render before a `--screenshot` is taken.
const SCREENSHOT_AFTER_FRAMES: u64 = 8;

pub struct Shell {
    core: Core,
    /// What the core may ask about the window, and what it asked for.
    host: Rc<RefCell<HostShared>>,
    window: Option<Window>,
    renderer: Option<Renderer>,
    ui: Ui,
    ui_gpu: Option<UiGpu>,
    ui_scale: f32,
    exit_requested: bool,
    /// The program could not start (see `fail_startup`).
    failed: bool,
    controls: Controls,
    pointer: Pointer,
    last_frame: Instant,
    /// When a picture of the video was last drawn (for the preset's rate cap).
    last_video_shown: Instant,
    /// When the renderer was last rebuilt after losing the device.
    last_graphics_try: Instant,
    needs_redraw: bool,
    cursor_pointer: bool,
    cursor_hidden: bool,
    animations: bool,
    stats_panel: StatsPanel,
    /// The side panel of the Studio mode.
    studio: StudioPanel,
    toasts: ToastHost,
}

impl Shell {
    pub fn new(mut core: Core, host: Rc<RefCell<HostShared>>) -> Self {
        let preview = core.cli().ui_preview;
        // Screenshots must show the settled state, not a mid-transition frame.
        let animations = core.cli().screenshot.is_none() && system_animations_enabled();
        let mut controls = Controls::new(preview);
        controls.set_animations(animations);
        let mut studio = StudioPanel::new();
        studio.set_animations(animations);
        let section = core.cli().studio_section.clone();
        if let Some(section) = section.as_deref().and_then(StudioSection::from_name) {
            studio.unfold(section);
        }
        open_requested_popover(&mut core, &mut controls);
        Self {
            core,
            host,
            window: None,
            renderer: None,
            ui: Ui::new(),
            ui_gpu: None,
            ui_scale: 1.0,
            exit_requested: false,
            failed: false,
            controls,
            pointer: Pointer::default(),
            last_frame: Instant::now(),
            last_video_shown: Instant::now(),
            last_graphics_try: Instant::now(),
            needs_redraw: true,
            cursor_pointer: false,
            cursor_hidden: false,
            animations,
            stats_panel: StatsPanel::new(),
            studio,
            toasts: ToastHost::new(),
        }
    }

    /// Whether the program could not start; the caller then exits with an
    /// error code.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// The window or Direct3D could not start: tells the user and stops.
    pub(super) fn fail_startup(&mut self, failure: StartupFailure, detail: &str) {
        self.failed = true;
        let silent = self.core.cli().screenshot.is_some();
        report_startup_failure(self.core.lang(), failure, detail, silent);
    }

    /// Does what the core asked of the window since the last call.
    fn apply_host_requests(&mut self) {
        let requests = std::mem::take(&mut self.host.borrow_mut().requests);
        for request in requests {
            match request {
                HostRequest::Fullscreen(on) => {
                    if let Some(window) = &self.window {
                        window.set_fullscreen(
                            on.then_some(winit::window::Fullscreen::Borderless(None)),
                        );
                    }
                }
                HostRequest::Preset(preset) => {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_filter(filter_for(preset));
                    }
                }
                HostRequest::Redraw => self.needs_redraw = true,
            }
        }
    }

    /// Tells the core what it may ask about the window.
    fn refresh_host_info(&mut self) {
        let mut shared = self.host.borrow_mut();
        if let Some(window) = &self.window {
            let size = window.inner_size();
            shared.window_size = (size.width, size.height);
            shared.monitor_hz = window
                .current_monitor()
                .and_then(|m| m.refresh_rate_millihertz())
                .map(|mhz| mhz as f32 / 1000.0);
        }
        shared.gpu_ms = self.renderer.as_ref().and_then(Renderer::gpu_ms);
    }

    /// Shows what the core says happened.
    fn show_events(&mut self) {
        for event in self.core.take_events() {
            let (key, kind) = toast_for(event);
            self.toast(key, kind);
        }
    }

    fn toast(&mut self, key: i18n::Key, kind: ToastKind) {
        let lang = self.core.lang();
        self.toasts.show(i18n::tr(lang, key), kind);
        self.needs_redraw = true;
    }
}

/// Opens the popover asked for on the command line (for screenshots).
fn open_requested_popover(core: &mut Core, controls: &mut Controls) {
    let (settings, replay, sources, scenes) = {
        let cli = core.cli();
        (
            cli.open_settings,
            cli.open_replay,
            cli.open_sources,
            cli.open_scenes,
        )
    };
    if settings {
        controls.toggle_settings();
    }
    if replay {
        controls.toggle_replay();
    }
    if sources {
        core.refresh_sources();
        controls.toggle_sources();
    }
    if scenes {
        core.refresh_sources();
        controls.toggle_scenes();
    }
}
