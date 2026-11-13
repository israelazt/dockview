//! The application: the window, the sources and the services, connected.
//! Each file here is one responsibility of `App`; the decisions themselves live
//! in the modules next to this one (`replay_service`, `auto_switch`,
//! `telemetry`, the scene modules) and are tested without a window.

mod actions;
mod events;
mod helpers;
mod replay;
mod scene_actions;
mod scene_runtime;
mod scene_transition;
mod sources;
mod startup;
mod stats;
mod window;

pub use startup::run;

use std::ffi::c_void;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dockview_domain::audio_status::AudioStatus;
use dockview_domain::device::{match_audio_to_video, pick_microphone, AudioDevice, VideoDevice};
use dockview_domain::device_names;
use dockview_domain::diagnostics::{diagnose, Finding, Snapshot, VideoState};
use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::ports::ReplayMachine;
use dockview_domain::recording::{RecorderStatus, RecordingInfo, RecordingState, ReplaySettings};
use dockview_domain::scene::SceneLayer;
use dockview_domain::settings::{
    self, AppSettings, LangMode, RenderPreset, SettingsStore, ThemeMode, VideoModePref,
};
use dockview_domain::stream::StreamInfo;
use dockview_platform::audio::{default_capture_device, enumerate_audio_devices, AudioEngine};
use dockview_platform::capture::{CaptureOptions, CaptureStatus, VideoCapture};
use dockview_platform::compose::SceneCompositor;
use dockview_platform::mf::{enumerate_video_devices, MfSession};
use dockview_platform::replay_machine::PlatformReplay;
use dockview_platform::screen::{
    list_monitors, list_windows, CaptureTarget, MonitorInfo, ScreenCapture, ScreenOptions,
    ScreenStatus, WindowInfo,
};
use dockview_platform::settings_store::FileSettingsStore;
use dockview_platform::tap_capture::{CaptureSource, TapCapture as MicCapture};
use dockview_platform::testsrc::TestSource;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::Key;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{CursorIcon, Fullscreen, Theme as WindowTheme, Window, WindowId};

use crate::auto_switch::{AutoSwitch, SwitchEvent};
use crate::cli::Cli;
use crate::keys::{self, KeyCommand};
use crate::replay_service::{RecordToggle, ReplayService};
use crate::sources::{
    find_window, monitor_label, pick_audio, pick_video, startup_scene, status_kind,
    video_state_of_capture, video_state_of_screen, AudioRoute, Reconnect, SourceKind, StartupScene,
};
use crate::stats_log;
use crate::telemetry::{Rates, Telemetry};
use dockview_domain::i18n::{self, Lang, LangModeExt};
use dockview_domain::stats::Stats;
use dockview_domain::theme::{Appearance, Theme, ThemeModeExt};
use dockview_domain::view::{
    Action, MicState, MicView, Model, RecState, RecView, ReplayState, SourcesView,
};
use dockview_platform::render::{self, Filter, Renderer};
use dockview_platform::ui_gpu::UiGpu;
use dockview_platform::{clips, hotkey, perf};
use dockview_ui::controls::Controls;
use dockview_ui::input::Pointer;
use dockview_ui::stats::StatsPanel;
use dockview_ui::toast::{ToastHost, ToastKind};
use dockview_ui::{self as ui, Ui};
use helpers::{
    appearance_of, filter_for, mic_device_id, mic_index_of, next_index, next_preset,
    on_helper_thread, resolution_label, system_animations_enabled,
};
use scene_runtime::Overlay;
use scene_transition::ActiveTransition;

/// How often the lists of monitors and windows are refreshed while shown.
const SOURCES_REFRESH: Duration = Duration::from_millis(1500);
/// How long the event loop waits for a new frame before checking events.
const FRAME_WAIT: Duration = Duration::from_millis(4);
/// Frames to render before a `--screenshot` is taken.
const SCREENSHOT_AFTER_FRAMES: u64 = 8;
/// Volume change per key press.
const VOLUME_STEP: f32 = 0.05;
/// Audio offset change per key press, in milliseconds.
const OFFSET_STEP_MS: i32 = 10;

enum Source {
    /// The handle only keeps the generator thread alive.
    Test(#[allow(dead_code)] TestSource),
    Capture {
        capture: VideoCapture,
        reconnect: Reconnect,
    },
    /// A monitor or a window, captured by Windows Graphics Capture.
    Screen(ScreenCapture),
    None,
}

struct App {
    /// What the viewer, the replay and the recording read.
    slot: Arc<FrameSlot>,
    /// Where the main source publishes. It forwards to `slot` unless a scene
    /// is running, when it keeps the frames for the compositor (see
    /// `scene_runtime`).
    input_slot: Arc<FrameSlot>,
    /// Draws the layers of the scene; `Some` only while there is a scene.
    scene: Option<SceneCompositor>,
    /// The layers above the base of the active scene, with their sources.
    overlays: Vec<Overlay>,
    /// The base layer of the active scene (its source is `source`).
    scene_base: Option<SceneLayer>,
    /// A change from one scene to another in progress (see `scene_transition`).
    transition: Option<ActiveTransition>,
    source: Source,
    video_devices: Vec<VideoDevice>,
    audio_devices: Vec<AudioDevice>,
    video_index: Option<usize>,
    audio_index: Option<usize>,
    audio: Option<AudioEngine>,
    settings: AppSettings,
    /// Where the settings are saved (the real file, or a stand-in in tests).
    store: Box<dyn SettingsStore>,
    window: Option<Window>,
    renderer: Option<Renderer>,
    rendered: u64,
    video_size: (u32, u32),
    fullscreen: bool,
    cli: Cli,
    ui: Ui,
    ui_gpu: Option<UiGpu>,
    ui_scale: f32,
    system_appearance: Appearance,
    system_lang: Lang,
    exit_requested: bool,
    controls: Controls,
    pointer: Pointer,
    last_frame: Instant,
    needs_redraw: bool,
    cursor_pointer: bool,
    cursor_hidden: bool,
    animations: bool,
    stats_open: bool,
    stats_panel: StatsPanel,
    stats: Stats,
    stats_log: Option<stats_log::StatsLog>,
    /// The once-a-second counters (see `telemetry`).
    telemetry: Telemetry,
    /// The automatic move to a compressed mode (see `auto_switch`).
    auto: AutoSwitch,
    /// Instant replay, clips, recording and the microphone (see `replay_service`).
    replay: ReplayService,
    toasts: ToastHost,
    /// Installed RAM, which caps the replay length.
    total_ram: u64,
    /// Whether the card, a monitor or a window is being captured.
    source_kind: SourceKind,
    /// The sound of the PC or of one program, while a screen is captured.
    loopback: Option<MicCapture>,
    /// What can be captured, refreshed while the source popover is open.
    monitors: Vec<MonitorInfo>,
    windows: Vec<WindowInfo>,
    sources_refreshed: Instant,
}
