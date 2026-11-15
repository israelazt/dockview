//! The window and everything that lives in it: the video renderer, the
//! interface, the pointer and the cursor. It feeds the application core what
//! happens (frames, keys, clicks, ticks) and draws what the core shows.

mod capture;
mod cursor;
mod events;
mod frame;
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
use dockview_domain::chrome_input::{ChromeInput, WindowCommand};
use dockview_domain::i18n;
use dockview_domain::presenter::Presenter;
use dockview_domain::window_chrome::{Chrome, Zone};
use dockview_ui::controls::Controls;
use dockview_ui::hud::Hud;
use dockview_ui::input::Pointer;
use dockview_ui::navbar::NavBar;
use dockview_ui::rail::Rail;
use dockview_ui::scene_editor::SceneEditor;
use dockview_ui::stats::StatsPanel;
use dockview_ui::strip::ShortcutStrip;
use dockview_ui::studio::{StudioPanel, StudioSection};
use dockview_ui::toast::{ToastHost, ToastKind};
use dockview_ui::Ui;
use winit::window::{CursorIcon, Window};

use crate::os::system_animations_enabled;
use helpers::filter_for;
use host::{HostRequest, HostShared};
use present::{report_startup_failure, toast_for, StartupFailure};

/// How long the event loop waits for a new frame before checking events.
const FRAME_WAIT: Duration = Duration::from_millis(4);
/// How often the application is ticked while the window is hidden in the tray
/// (the hotkey and the icon are polled then).
const HIDDEN_TICK: Duration = Duration::from_millis(50);
/// How long to wait before trying to rebuild a renderer that could not be built.
const GRAPHICS_RETRY: Duration = Duration::from_secs(1);
/// Frames to render before a `--screenshot` is taken.
const SCREENSHOT_AFTER_FRAMES: u64 = 8;

pub struct Shell {
    core: Core,
    /// What the core may ask about the window, and what it asked for.
    host: Rc<RefCell<HostShared>>,
    window: Option<Window>,
    /// The window's graphics (the video and the interface on top).
    presenter: Option<Box<dyn Presenter>>,
    ui: Ui,
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
    cursor_icon: CursorIcon,
    /// The layer editor over the picture, and the keys that modify its drags.
    scene_editor: SceneEditor,
    /// The window is hidden: Dockview is only in the tray.
    hidden: bool,
    edit_mods: dockview_domain::layer_edit::Modifiers,
    /// Ctrl is held (it turns letters into chords).
    ctrl: bool,
    cursor_hidden: bool,
    animations: bool,
    stats_panel: StatsPanel,
    /// The side panel of the Studio mode.
    studio: StudioPanel,
    /// The tabs and the status under the title bar.
    navbar: NavBar,
    /// The icon rail on the left.
    rail: Rail,
    /// The shortcuts above the content.
    strip: ShortcutStrip,
    /// The readouts over the picture in the viewer.
    hud: Hud,
    /// The readouts are fading in or out (so frames keep being drawn).
    hud_moving: bool,
    /// How far the Studio panel was out when the frame was drawn.
    studio_openness: f32,
    toasts: ToastHost,
    /// What the pointer is doing to the window's own frame (its buttons, its
    /// edges and its bar).
    chrome_input: ChromeInput,
    /// Where the pointer is, in physical pixels (the frame works in those).
    cursor_phys: Option<(f32, f32)>,
    /// The press that is going on began on the frame, so the content never saw it.
    chrome_held: bool,
    /// When the shell started: the clock the double click is measured on.
    started: Instant,
}

impl Shell {
    pub fn new(core: Core, host: Rc<RefCell<HostShared>>) -> Self {
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
        Self {
            core,
            host,
            window: None,
            presenter: None,
            ui: Ui::new(),
            ui_scale: 1.0,
            exit_requested: false,
            failed: false,
            controls,
            pointer: Pointer::default(),
            last_frame: Instant::now(),
            last_video_shown: Instant::now(),
            last_graphics_try: Instant::now(),
            needs_redraw: true,
            cursor_icon: CursorIcon::Default,
            scene_editor: SceneEditor::new(),
            hidden: false,
            edit_mods: Default::default(),
            ctrl: false,
            cursor_hidden: false,
            animations,
            stats_panel: StatsPanel::new(),
            studio,
            navbar: NavBar::new(),
            rail: Rail::new(),
            strip: ShortcutStrip::new(),
            hud: Hud::new(),
            hud_moving: false,
            studio_openness: 0.0,
            toasts: ToastHost::new(),
            chrome_input: ChromeInput::default(),
            cursor_phys: None,
            chrome_held: false,
            started: Instant::now(),
        }
    }

    /// The window's own frame as it is now (physical pixels).
    pub(super) fn chrome(&self) -> Chrome {
        let (size, maximized) = self.window.as_ref().map_or(((1, 1), false), |w| {
            let size = w.inner_size();
            ((size.width, size.height), w.is_maximized())
        });
        Chrome {
            size,
            scale: self.ui_scale,
            maximized,
            fullscreen: self.core.fullscreen(),
            rail_out: self.studio.openness(),
        }
    }

    /// Carries out what the frame's buttons, bar or edges asked of the window.
    pub(super) fn run_window_command(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        command: WindowCommand,
    ) {
        let Some(window) = &self.window else {
            return;
        };
        match command {
            WindowCommand::Minimize => window.set_minimized(true),
            WindowCommand::ToggleMaximize => window.set_maximized(!window.is_maximized()),
            WindowCommand::Close => self.request_close(event_loop),
            WindowCommand::Drag => {
                let _ = window.drag_window();
            }
            WindowCommand::Resize(edge) => {
                let _ = window.drag_resize_window(cursor::resize_direction(edge));
            }
            WindowCommand::Menu => {
                if let Some((x, y)) = self.cursor_phys {
                    window.show_window_menu(winit::dpi::PhysicalPosition::new(
                        f64::from(x),
                        f64::from(y),
                    ));
                }
            }
        }
        self.needs_redraw = true;
    }

    /// The window was asked to close: to the tray, or the end of the program.
    pub(super) fn request_close(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        self.core.save_settings();
        if self.core.keeps_in_tray() {
            // The icon in the tray brings it back; the hotkey keeps working.
            self.hide_window();
        } else {
            event_loop.exit();
        }
    }

    /// The pointer as the navigation row sees it: relative to the row, and only
    /// while it is over the row.
    pub(super) fn nav_pointer(&self, chrome: &Chrome) -> Pointer {
        let over_nav = self
            .cursor_phys
            .is_some_and(|(x, y)| chrome.zone_at(x, y) == Zone::Nav);
        let bar = chrome.bar_height() / self.ui_scale.max(0.1);
        Pointer {
            pos: if over_nav && !self.chrome_held {
                self.pointer.pos.map(|(x, y)| (x, y - bar))
            } else {
                None
            },
            ..self.pointer
        }
    }

    /// The pointer as the icon rail sees it: relative to the rail, and only
    /// while it is over it.
    pub(super) fn rail_pointer(&self, chrome: &Chrome) -> Pointer {
        let over_rail = self
            .cursor_phys
            .is_some_and(|(x, y)| chrome.zone_at(x, y) == Zone::Rail);
        let top = (chrome.bar_height() + chrome.nav_height()) / self.ui_scale.max(0.1);
        // While it slides, its icons are that much to the left.
        let slide =
            dockview_domain::window_chrome::RAIL_WIDTH * (1.0 - chrome.rail_out.clamp(0.0, 1.0));
        Pointer {
            pos: if over_rail && !self.chrome_held {
                self.pointer.pos.map(|(x, y)| (x + slide, y - top))
            } else {
                None
            },
            ..self.pointer
        }
    }

    /// The pointer as the content sees it: below the title bar, and gone while
    /// it is over the frame (the bar, its buttons or a resize edge).
    pub(super) fn content_pointer(&self, chrome: &Chrome) -> Pointer {
        let over_frame = self
            .cursor_phys
            .is_some_and(|(x, y)| chrome.zone_at(x, y) != Zone::Content);
        let scale = self.ui_scale.max(0.1);
        let bar = (chrome.bar_height() + chrome.nav_height() + chrome.strip_height()) / scale;
        let rail = chrome.rail_width() / scale;
        Pointer {
            pos: if over_frame || self.chrome_held {
                None
            } else {
                self.pointer.pos.map(|(x, y)| (x - rail, y - bar))
            },
            ..self.pointer
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
                    if let Some(presenter) = self.presenter.as_mut() {
                        presenter.set_filter(filter_for(preset));
                    }
                }
                HostRequest::Redraw => self.needs_redraw = true,
                HostRequest::Capture(mode) => self.start_capture(mode),
                HostRequest::ShowWindow => self.show_window(),
                HostRequest::Quit => {
                    self.core.save_settings();
                    self.exit_requested = true;
                }
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
        shared.gpu_ms = self.presenter.as_ref().and_then(|p| p.gpu_ms());
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
