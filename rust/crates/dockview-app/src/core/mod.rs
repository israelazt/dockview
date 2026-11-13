//! The application core. Each file here is one responsibility of `Core`; the
//! decisions themselves live in the modules next to this one (`replay_service`,
//! `auto_switch`, `telemetry`, `sources`) and are tested without a window.

/// What the scenes service needs to run the compositor, borrowed from the
/// fields it lives in, so `self.scenes` can be borrowed mutably at the same time.
macro_rules! scene_env {
    ($core:expr) => {
        $crate::scenes_service::SceneEnv {
            machine: &*$core.sources.machine,
            input_slot: &$core.sources.input_slot,
            slot: &$core.slot,
        }
    };
}

mod actions;
mod api;
mod devices;
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
use dockview_domain::device::{pick_microphone, AudioDevice, DeviceLists, VideoDevice};
use dockview_domain::device_names;
use dockview_domain::diagnostics::{diagnose, Finding, Snapshot, VideoState};
use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::i18n::{self, Lang, LangModeExt};
use dockview_domain::ports::{Host, ReplayMachine, SourceMachine, StatsSink, SystemProbe};
use dockview_domain::recording::{RecorderStatus, RecordingInfo, RecordingState, ReplaySettings};
use dockview_domain::settings::{
    self, AppSettings, LangMode, RenderPreset, SettingsStore, ThemeMode, VideoModePref,
};
use dockview_domain::sources::{CaptureOptions, CaptureTarget, ScreenOptions};
use dockview_domain::stats::Stats;
use dockview_domain::stream::StreamInfo;
use dockview_domain::theme::{Appearance, Theme, ThemeModeExt};
use dockview_domain::view::{Action, MicState, MicView, Model, RecState, RecView, ReplayState};
use helpers::{mic_device_id, mic_index_of, next_index, next_preset, resolution_label};

use crate::auto_switch::{AutoSwitch, SwitchEvent};
use crate::cli::Cli;
use crate::keys::KeyCommand;
use crate::replay_service::{RecordToggle, ReplayService};
use crate::scenes_service::ScenesService;
use crate::settings_storage::SettingsStorage;
use crate::sources::{
    find_window, monitor_to_restore, pick_audio, pick_video, reconcile, startup_scene, status_kind,
    Reconnect, SourceKind, StartupScene,
};
use crate::sources_service::{Maintained, Source, SourcesService};
use crate::telemetry::{Rates, Telemetry};

/// How often the lists of monitors and windows are refreshed while shown.
pub const SOURCES_REFRESH: Duration = Duration::from_millis(1500);
/// Volume change per key press.
const VOLUME_STEP: f32 = 0.05;
/// Audio offset change per key press, in milliseconds.
const OFFSET_STEP_MS: i32 = 10;

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
    /// The scene that is running and the change to another (see
    /// `scenes_service`).
    scenes: ScenesService,
    /// The devices, the monitors and the windows, the main source and the
    /// machine they are started on (see `sources_service`).
    sources: SourcesService,
    settings: AppSettings,
    /// When to write the settings to disk, and what to do when that fails.
    storage: SettingsStorage,
    /// What the window shows and knows about itself.
    view: ViewState,
    /// What is measured once a second and acted on.
    measures: Measurements,
    cli: Cli,
    /// Instant replay, clips, recording and the microphone (see `replay_service`).
    replay: ReplayService,
    /// Installed RAM, which caps the replay length.
    total_ram: u64,
    /// What happened since the last tick, to be shown (see `toast_for`).
    events: Vec<AppEvent>,
    /// What the application asks of the window (the real one, or none in tests).
    host: Box<dyn Host>,
}

/// What the window shows and knows about itself.
struct ViewState {
    /// Pictures shown so far.
    rendered: u64,
    /// The size of the last picture shown.
    video_size: (u32, u32),
    fullscreen: bool,
    /// The system's light or dark setting, which `auto` follows.
    system_appearance: Appearance,
    system_lang: Lang,
    stats_open: bool,
}

/// What is measured once a second and acted on (see `stats`).
struct Measurements {
    /// The last measurements, shown by the statistics panel.
    stats: Stats,
    /// Where they are logged, when asked for.
    sink: Option<Box<dyn StatsSink>>,
    /// The once-a-second counters (see `telemetry`).
    telemetry: Telemetry,
    /// The automatic move to a compressed mode (see `auto_switch`).
    auto: AutoSwitch,
}

impl Drop for Core {
    /// Whatever was changed last is not lost when the program ends.
    fn drop(&mut self) {
        if self.storage.is_dirty() {
            self.save_settings();
        }
    }
}
