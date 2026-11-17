//! The view model: what the screen shows (`Model`) and what the user asks for
//! (`Action`). The application builds the model; the interface draws it and
//! returns actions. Neither knows the other.

use crate::settings::{RecordingQuality, SceneTransition};

use crate::broadcast::StreamState;
use crate::i18n::Lang;
use crate::navigation::NavView;
use crate::scene::NormRect;
use crate::theme::Theme;

/// How healthy the video source is, for the status dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Live,
    Connecting,
    Problem,
    Idle,
}

/// State of the instant-replay buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayState {
    Off,
    Starting,
    Recording,
    /// This PC cannot record (no hardware encoder, unsupported format).
    Unavailable,
}

/// What is being shown and recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CurrentSource {
    /// The selected capture card.
    #[default]
    Capture,
    /// A monitor, by position in the list.
    Monitor(usize),
    /// A window, by position in the list (`None` if it is not listed any more).
    Window(Option<usize>),
}

/// The monitors and windows that can be captured.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourcesView {
    pub monitors: Vec<String>,
    pub windows: Vec<String>,
    pub current: CurrentSource,
}

/// State of a whole-session recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecState {
    Idle,
    /// Asked to start; the file begins at the next keyframe.
    Starting,
    Recording,
}

/// What the recording controls display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecView {
    pub state: RecState,
    /// Footage written so far, in seconds.
    pub seconds: f32,
}

impl RecView {
    pub const fn idle() -> Self {
        Self {
            state: RecState::Idle,
            seconds: 0.0,
        }
    }

    pub fn is_on(&self) -> bool {
        self.state != RecState::Idle
    }
}

/// State of the microphone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicState {
    Off,
    Starting,
    Live,
    /// No microphone could be opened.
    Unavailable,
}

/// What the microphone controls display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MicView {
    pub state: MicState,
    /// How loud it is right now, 0.0–1.0.
    pub level: f32,
}

impl MicView {
    pub const fn off() -> Self {
        Self {
            state: MicState::Off,
            level: 0.0,
        }
    }

    /// Turned on by the user (even if it could not start).
    pub fn is_on(&self) -> bool {
        self.state != MicState::Off
    }
}

/// What the replay controls display.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayView {
    pub state: ReplayState,
    /// A clip is being written right now.
    pub saving: bool,
    pub buffered_seconds: f32,
    pub buffered_mb: f32,
    /// Length of a clip, in seconds.
    pub keep_seconds: u32,
    /// Memory the buffer needs at this length, in bytes.
    pub memory_bytes: u64,
    /// The chosen length is the longest this PC's memory allows.
    pub at_limit: bool,
    /// Video bitrate of the buffer and the recordings, in Mbps.
    pub bitrate_mbps: u32,
    pub folder: String,
    /// The hotkey that saves a clip.
    pub hotkey: String,
    /// The hotkey that starts and stops a recording.
    pub record_hotkey: String,
}

impl ReplayView {
    /// A turned-off replay with the given hotkey label.
    pub fn off(hotkey: &str) -> Self {
        Self {
            state: ReplayState::Off,
            saving: false,
            buffered_seconds: 0.0,
            buffered_mb: 0.0,
            keep_seconds: 30,
            memory_bytes: 0,
            at_limit: false,
            bitrate_mbps: 40,
            folder: String::new(),
            hotkey: hotkey.to_owned(),
            record_hotkey: String::new(),
        }
    }

    /// Whether the buffer is on (recording or starting up).
    pub fn is_on(&self) -> bool {
        matches!(self.state, ReplayState::Starting | ReplayState::Recording)
    }
}

/// What the stream controls display.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreamView {
    pub state: StreamState,
    /// Bitrate actually sent, in kilobits per second.
    pub bitrate_kbps: u32,
    pub dropped_frames: u64,
    pub reconnects: u32,
    /// How the connection is doing (judged over the last few seconds).
    pub health: crate::stream_health::StreamHealth,
    /// A stream key is saved (the key itself never reaches the interface).
    pub has_key: bool,
    /// The server address chosen.
    pub address: String,
    /// Frames per second and video bitrate (kilobits per second) chosen.
    pub fps: u32,
    pub video_kbps: u32,
}

/// Everything the controls display.
#[derive(Debug, Clone)]
pub struct Model {
    pub theme: Theme,
    pub lang: Lang,
    pub status: StatusKind,
    /// e.g. `1080p · 60 fps`; empty until the first frame arrives.
    pub resolution: String,
    pub volume: f32,
    pub sync_ms: i32,
    pub fullscreen: bool,
    pub video_devices: Vec<String>,
    pub video_selected: Option<usize>,
    pub audio_devices: Vec<String>,
    pub audio_selected: Option<usize>,
    /// The first entry is the Windows default microphone.
    pub mic_devices: Vec<String>,
    pub mic_selected: Option<usize>,
    /// What the Settings mode shows.
    pub settings: crate::settings_view::SettingsView,
    /// The mode that is on. Full screen shows only the picture without
    /// leaving the mode.
    pub mode: crate::navigation::AppMode,
    /// The screenshot tool is on (see `snap::ENABLED`).
    pub snip: bool,
    pub replay: ReplayView,
    pub mic: MicView,
    pub rec: RecView,
    pub sources: SourcesView,
    pub scenes: ScenesView,
    pub stream: StreamView,
    pub nav: NavView,
    /// The row of readouts over the picture is on.
    pub hud: bool,
}

impl Model {
    /// Whether the Studio panel is on screen: full screen shows only the video.
    pub fn studio_visible(&self) -> bool {
        self.mode.shows_panel() && !self.fullscreen
    }
}

/// A change the user asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    SetVolume(f32),
    SetSync(i32),
    ToggleFullscreen,
    SelectVideo(usize),
    SelectAudio(usize),
    SelectMic(usize),
    /// Capture this monitor (position in the list).
    SelectMonitor(usize),
    /// Capture this window (position in the list).
    SelectWindow(usize),
    ToggleMic,
    /// Start or stop recording the session.
    ToggleRecord,
    /// A change in the Settings mode.
    Settings(crate::settings_view::SettingsAction),
    SetSceneTransition(SceneTransition),
    /// How long a smooth scene change takes, in milliseconds.
    SetTransitionMs(u32),
    /// Turn the layer editor's pull to edges and centres on or off.
    ToggleCanvasSnap,
    /// Show or hide the grid of the layer editor.
    ToggleCanvasGrid,
    /// Open the Studio on its statistics.
    OpenStats,
    /// Open or close the Studio side panel.
    ToggleStudio,
    /// Open the Studio panel (and leave full screen to show it); it stays
    /// open if it already was.
    OpenStudio,
    ToggleReplay,
    /// Go live, or end the stream.
    ToggleStream,
    /// Show or hide the row of readouts over the picture.
    ToggleHud,
    /// Take a screenshot in the mode chosen in the settings.
    Capture,
    /// Go to a tab of the navigation row.
    GoTo(crate::navigation::AppMode),
    /// The server address the stream goes to.
    SetStreamUrl(String),
    /// The stream key, which is kept apart from the settings.
    SetStreamKey(String),
    /// Frames per second of the stream.
    SetStreamFps(u32),
    /// Video bitrate of the stream, in kilobits per second.
    SetStreamKbps(u32),
    /// Move the clip length one step longer (+1) or shorter (-1).
    ReplayLength(i32),
    SaveReplay,
    OpenReplayFolder,
    /// Choose another folder for clips and recordings.
    ChooseFolder,
    SetQuality(RecordingQuality),
    /// A change in the scenes popover.
    Scene(SceneAction),
    /// Give the scene at this position the name that was typed.
    RenameScene(usize, String),
}

/// A change asked for in the scenes popover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneAction {
    /// Use this scene, or with `None` the plain source.
    Activate(Option<usize>),
    /// Make a scene out of the current source and use it.
    New,
    Delete(usize),
    /// Show or hide a layer of the active scene.
    ToggleLayer(usize),
    /// Lock or unlock a layer of the active scene (a locked one cannot be
    /// picked or moved over the picture).
    ToggleLock(usize),
    RemoveLayer(usize),
    /// Add the capture card, monitor or window at this position of its list.
    AddCapture(usize),
    AddMonitor(usize),
    AddWindow(usize),
    /// Place the layer at this position: `[x, y, width, height]` in
    /// ten-thousandths of the picture (see `layer_edit::to_units`).
    SetLayerRect(usize, [i32; 4]),
    /// Move the layer one step up (+1) or down (-1) in the drawing order. The
    /// base always stays at the bottom.
    MoveLayer(usize, i32),
    /// Choose the layer the editor works on (or none).
    SelectLayer(Option<usize>),
    /// Turn the layer editor over the picture on or off.
    ToggleEdit,
    /// The editor takes hold of this layer: its rectangle is what Undo goes
    /// back to if the layer ends up somewhere else.
    BeginEdit(usize),
    /// Put the last edited layer back where it was.
    Undo,
    /// Stretch the chosen layer over the whole picture.
    FitLayer,
}

/// What kind of source a layer shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayerKind {
    #[default]
    Capture,
    Monitor,
    Window,
}

/// One layer as the popover shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerView {
    pub name: String,
    pub kind: LayerKind,
    pub visible: bool,
    /// Locked: the editor over the picture leaves it alone.
    pub locked: bool,
    pub rect: NormRect,
}

/// The scenes and the layers of the one in use.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScenesView {
    pub names: Vec<String>,
    /// How many layers each scene has, in the order of `names`.
    pub layer_counts: Vec<usize>,
    pub active: Option<usize>,
    /// The first one is the base.
    pub layers: Vec<LayerView>,
    /// Whether another layer fits in the active scene.
    pub can_add: bool,
    /// The layer the editor works on.
    pub selected: Option<usize>,
    /// The editor is on, over the picture.
    pub editing: bool,
    /// How the picture changes between scenes.
    pub transition: SceneTransition,
    /// How long a smooth change takes, in milliseconds.
    pub transition_ms: u32,
    /// Layers are pulled to edges, centres and other layers while edited.
    pub snap: bool,
    /// The grid is shown and layers are pulled to its lines.
    pub grid: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(studio: bool, fullscreen: bool) -> Model {
        use crate::navigation::AppMode;
        Model {
            theme: Theme::dark(),
            lang: Lang::En,
            status: StatusKind::Live,
            resolution: String::new(),
            volume: 1.0,
            sync_ms: 0,
            fullscreen,
            video_devices: vec![],
            video_selected: None,
            audio_devices: vec![],
            audio_selected: None,
            mic_devices: vec![],
            mic_selected: None,
            settings: crate::settings_view::SettingsView::default(),
            mode: if studio {
                AppMode::Studio
            } else {
                AppMode::Viewer
            },
            snip: true,
            replay: ReplayView::off("Ctrl+Alt+S"),
            mic: MicView::off(),
            rec: RecView::idle(),
            sources: SourcesView::default(),
            scenes: ScenesView::default(),
            stream: StreamView::default(),
            nav: NavView::default(),
            hud: true,
        }
    }

    #[test]
    fn the_studio_panel_shows_only_when_asked_for() {
        assert!(model(true, false).studio_visible());
        assert!(!model(false, false).studio_visible());
    }

    #[test]
    fn full_screen_hides_the_studio_panel_without_closing_it() {
        let m = model(true, true);
        assert!(!m.studio_visible());
        assert!(m.mode.shows_panel(), "leaving full screen brings it back");
    }
}
