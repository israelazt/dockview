//! The application core. Each file here is one responsibility of `Core`; the
//! decisions themselves live in the modules next to this one (`replay_service`,
//! `auto_switch`, `telemetry`, `sources`) and are tested without a window.

mod actions;
mod api;
mod helpers;
mod overlays;
mod replay;
mod scene_actions;
mod scene_runtime;
mod scene_transition;
mod sources;
mod startup;
mod stats;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::{Duration, Instant};

use dockview_domain::audio_status::AudioStatus;
use dockview_domain::device::{match_audio_to_video, pick_microphone, AudioDevice, VideoDevice};
use dockview_domain::device_names;
use dockview_domain::diagnostics::{diagnose, Finding, Snapshot, VideoState};
use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::i18n::{self, Lang, LangModeExt};
use dockview_domain::ports::{
    AudioPlayback, CaptureHandle, Host, Loopback, MicInput, ReplayMachine, SceneCompositor,
    ScreenHandle, SourceMachine, StatsSink, SystemProbe,
};
use dockview_domain::recording::{RecorderStatus, RecordingInfo, RecordingState, ReplaySettings};
use dockview_domain::scene::SceneLayer;
use dockview_domain::settings::{
    self, AppSettings, LangMode, RenderPreset, SettingsStore, ThemeMode, VideoModePref,
};
use dockview_domain::sources::{
    CaptureOptions, CaptureStatus, CaptureTarget, MonitorInfo, ScreenOptions, ScreenStatus,
    WindowInfo,
};
use dockview_domain::stats::Stats;
use dockview_domain::stream::StreamInfo;
use dockview_domain::theme::{Appearance, Theme, ThemeModeExt};
use dockview_domain::view::{
    Action, MicState, MicView, Model, RecState, RecView, ReplayState, SourcesView,
};
use helpers::{mic_device_id, mic_index_of, next_index, next_preset, resolution_label};
use scene_runtime::Overlay;
use scene_transition::ActiveTransition;

use crate::auto_switch::{AutoSwitch, SwitchEvent};
use crate::cli::Cli;
use crate::keys::KeyCommand;
use crate::replay_service::{RecordToggle, ReplayService};
use crate::sources::{
    find_window, monitor_label, pick_audio, pick_video, startup_scene, status_kind,
    video_state_of_capture, video_state_of_screen, AudioRoute, Reconnect, SourceKind, StartupScene,
};
use crate::telemetry::{Rates, Telemetry};

/// How often the lists of monitors and windows are refreshed while shown.
pub const SOURCES_REFRESH: Duration = Duration::from_millis(1500);
/// Volume change per key press.
const VOLUME_STEP: f32 = 0.05;
/// Audio offset change per key press, in milliseconds.
const OFFSET_STEP_MS: i32 = 10;

enum Source {
    /// The handle only keeps the generator thread alive.
    Test(#[allow(dead_code)] Box<dyn std::any::Any>),
    Capture {
        capture: Box<dyn CaptureHandle>,
        reconnect: Reconnect,
    },
    /// A monitor or a window, captured by Windows Graphics Capture.
    Screen(Box<dyn ScreenHandle>),
    None,
}

/// Everything the application needs from the machine, behind ports. The real
/// ones are made by the binary; tests pass fakes.
pub struct Ports {
    /// Where the settings are saved.
    pub settings: Box<dyn SettingsStore>,
    /// The replay buffer, the microphone, the hotkeys and the clips folder.
    pub replay: Box<dyn ReplayMachine>,
    /// The capture cards, screens, audio and the scene compositor.
    pub sources: Box<dyn SourceMachine>,
    /// CPU, memory and installed RAM.
    pub system: Arc<dyn SystemProbe>,
    /// Where the measurements are logged, when asked for.
    pub stats_sink: Option<Box<dyn StatsSink>>,
    /// The window.
    pub host: Box<dyn Host>,
}

/// The state and the services of the application. The binary creates one,
/// feeds it what happens (frames, ticks, actions, keys) and draws its model.
pub struct Core {
    /// What the viewer, the replay and the recording read.
    slot: Arc<FrameSlot>,
    /// Where the main source publishes. It forwards to `slot` unless a scene
    /// is running, when it keeps the frames for the compositor (see
    /// `scene_runtime`).
    input_slot: Arc<FrameSlot>,
    /// Draws the layers of the scene; `Some` only while there is a scene.
    scene: Option<Box<dyn SceneCompositor>>,
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
    audio: Option<Box<dyn AudioPlayback>>,
    settings: AppSettings,
    /// Where the settings are saved (the real file, or a stand-in in tests).
    store: Box<dyn SettingsStore>,
    rendered: u64,
    video_size: (u32, u32),
    fullscreen: bool,
    cli: Cli,
    system_appearance: Appearance,
    system_lang: Lang,
    stats_open: bool,
    stats: Stats,
    stats_sink: Option<Box<dyn StatsSink>>,
    /// The once-a-second counters (see `telemetry`).
    telemetry: Telemetry,
    /// The automatic move to a compressed mode (see `auto_switch`).
    auto: AutoSwitch,
    /// Instant replay, clips, recording and the microphone (see `replay_service`).
    replay: ReplayService,
    /// Installed RAM, which caps the replay length.
    total_ram: u64,
    /// Whether the card, a monitor or a window is being captured.
    source_kind: SourceKind,
    /// The sound of the PC or of one program, while a screen is captured.
    loopback: Option<Box<dyn MicInput>>,
    /// What the sources are started on (the real machine, or a fake in tests).
    machine: Box<dyn SourceMachine>,
    /// What happened since the last tick, to be shown (see `toast_for`).
    events: Vec<AppEvent>,
    /// What can be captured, refreshed while the source popover is open.
    monitors: Vec<MonitorInfo>,
    windows: Vec<WindowInfo>,
    sources_refreshed: Instant,
    /// What the application asks of the window (the real one, or none in tests).
    host: Box<dyn Host>,
}
