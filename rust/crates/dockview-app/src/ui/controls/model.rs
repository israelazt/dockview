//! What the controls display (`Model`) and what they ask for (`Action`).

use dockview_core::settings::{LangMode, RecordingQuality, RenderPreset, VideoModePref};

use crate::i18n::Lang;
use crate::theme::{Theme, ThemeMode};

use super::scenes::{SceneAction, ScenesView};

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
    pub preset: RenderPreset,
    pub theme_mode: ThemeMode,
    pub lang_mode: LangMode,
    pub video_mode: VideoModePref,
    pub stats_open: bool,
    pub replay: ReplayView,
    pub mic: MicView,
    pub rec: RecView,
    pub sources: SourcesView,
    pub scenes: ScenesView,
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
    SetPreset(RenderPreset),
    SetTheme(ThemeMode),
    SetLang(LangMode),
    SetVideoMode(VideoModePref),
    ToggleStats,
    ToggleReplay,
    /// Move the clip length one step longer (+1) or shorter (-1).
    ReplayLength(i32),
    SaveReplay,
    OpenReplayFolder,
    /// Choose another folder for clips and recordings.
    ChooseFolder,
    SetQuality(RecordingQuality),
    /// A change in the scenes popover.
    Scene(SceneAction),
}
