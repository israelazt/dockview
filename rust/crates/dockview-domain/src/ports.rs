//! The ports: what the application needs from the machine, said as traits.
//!
//! The services in `dockview-app` work only against these. The real
//! implementations (the GPU encoder, the microphone, the global hotkeys, the
//! shell) live in `dockview-platform`; tests plug in fakes, so a service can be
//! tested without a capture card, a GPU or a window.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::audio_status::AudioStatus;
use crate::device::AudioDevice;
use crate::frame::FrameSlot;
use crate::pcm::AudioTap;
use crate::recording::{
    RecorderStatus, RecordingInfo, RecordingOutcome, ReplaySettings, SaveHandle,
};
use crate::sources::{
    CaptureOptions, CaptureStatus, CaptureTarget, Layer, MonitorInfo, ScreenOptions, ScreenStatus,
    WindowInfo,
};

/// The replay buffer while it runs, and the recording that rides on it.
pub trait ReplayBuffer {
    fn status(&self) -> RecorderStatus;
    /// Footage held now, in seconds and in bytes.
    fn buffered_seconds(&self) -> f64;
    fn buffered_bytes(&self) -> usize;
    /// How much footage to keep, in seconds.
    fn set_keep_seconds(&self, seconds: f64);
    /// Saves (at least) the last `seconds` to `path` in the background.
    fn save(&self, seconds: f64, path: PathBuf) -> SaveHandle;

    fn recording(&self) -> RecordingInfo;
    fn start_recording(&self, path: PathBuf) -> Result<(), String>;
    fn stop_recording(&self);
    /// How a recording (or one of its files) ended, once, in order.
    fn take_recording_result(&self) -> Option<RecordingOutcome>;

    /// Where the game's sound is delivered to be recorded.
    fn audio_tap(&self) -> Arc<AudioTap>;
    /// Where the microphone's sound is delivered to be recorded.
    fn mic_tap(&self) -> Arc<AudioTap>;
    /// Forgets the microphone's audio (it was turned off).
    fn clear_mic(&self);
}

/// An open microphone.
pub trait MicInput {
    fn status(&self) -> AudioStatus;
    /// Loudness of the last block, 0.0 to 1.0.
    fn level(&self) -> f32;
}

/// The global hotkeys of the replay: they work whatever window has the focus.
pub trait ReplayHotkeys {
    /// Whether "save a clip" was pressed since the last call.
    fn take_save_pressed(&self) -> bool;
    /// Whether "record" was pressed since the last call.
    fn take_record_pressed(&self) -> bool;
}

/// Everything the replay needs from the machine.
pub trait ReplayMachine {
    /// Starts the buffer over the pictures published to `slot`.
    fn start_buffer(&self, slot: Arc<FrameSlot>, settings: ReplaySettings)
        -> Box<dyn ReplayBuffer>;
    /// Opens a microphone (`None` is the Windows default input) feeding `tap`.
    fn open_mic(&self, device: Option<String>, tap: Arc<AudioTap>) -> Box<dyn MicInput>;
    /// Registers the global hotkeys. One that another program has is only
    /// reported (by the implementation); the rest keep working.
    fn register_hotkeys(&self) -> Box<dyn ReplayHotkeys>;

    /// Free bytes on the disk that holds `folder`, when it can be told.
    fn disk_free_bytes(&self, folder: &Path) -> Option<u64>;
    /// A name for a new clip in `folder` that no existing file has.
    fn new_clip_path(&self, folder: &Path) -> PathBuf;
    /// A name for a new recording in `folder` that no existing file has.
    fn new_recording_path(&self, folder: &Path) -> PathBuf;
    /// Makes sure `folder` exists; whether it does.
    fn create_folder(&self, folder: &Path) -> bool;
    /// Shows the newest saved file (if it still exists), else opens `folder`.
    fn show_saved(&self, newest: Option<&Path>, folder: &Path);
}

// ---------------------------------------------------------------------------
// The video and audio sources
// ---------------------------------------------------------------------------

/// A capture card (or camera) that is open. Dropping it closes the device.
pub trait CaptureHandle {
    fn status(&self) -> CaptureStatus;
}

/// A monitor or window being captured. Dropping it stops the capture.
pub trait ScreenHandle {
    fn status(&self) -> ScreenStatus;
}

/// The capture card's sound being played to the speakers. Dropping it stops it.
pub trait AudioPlayback {
    fn status(&self) -> AudioStatus;
    fn set_volume(&self, volume: f32);
    fn set_offset_ms(&self, offset_ms: i32);
    /// Starts (or with `None`, stops) feeding every block to the recorder.
    fn set_tap(&self, tap: Option<Arc<AudioTap>>);
    /// Audio buffered for the sync offset, in milliseconds.
    fn buffered_ms(&self) -> u32;
}

/// The drawing of several sources into one picture. Dropping it stops it.
pub trait SceneCompositor {
    /// Replaces the layers (a scene change or an edit).
    fn set_layers(&self, layers: Vec<Layer>);
}

/// Which sound to listen to when a screen is captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loopback {
    /// Everything the PC plays.
    System,
    /// What one program (and the programs it started) plays.
    Process(u32),
}

/// Everything the application needs to get pictures and sound from the machine.
pub trait SourceMachine {
    /// Opens the capture card with this device link, publishing to `slot`.
    fn start_capture(
        &self,
        symbolic_link: String,
        options: CaptureOptions,
        slot: Arc<FrameSlot>,
    ) -> Box<dyn CaptureHandle>;
    /// Captures a monitor or a window, publishing to `slot`.
    fn start_screen(
        &self,
        target: CaptureTarget,
        options: ScreenOptions,
        slot: Arc<FrameSlot>,
    ) -> Box<dyn ScreenHandle>;
    /// A synthetic picture (colour bars), for tests and previews. The handle
    /// only keeps it alive.
    fn start_test_pattern(&self, slot: Arc<FrameSlot>, fps: u32) -> Box<dyn std::any::Any>;

    fn list_monitors(&self) -> Vec<MonitorInfo>;
    fn list_windows(&self) -> Vec<WindowInfo>;

    /// Plays the sound of the audio device `device_id` to the speakers.
    fn start_playback(
        &self,
        device_id: String,
        volume: f32,
        offset_ms: i32,
    ) -> Box<dyn AudioPlayback>;
    /// Listens to what the PC (or one program) plays, feeding `tap`; it is not
    /// played back.
    fn start_loopback(&self, source: Loopback, tap: Arc<AudioTap>) -> Box<dyn MicInput>;
    /// The audio input Windows treats as the default, if there is one.
    fn default_capture_device(&self) -> Option<AudioDevice>;

    /// Draws `layers` into one picture of `width` x `height`, published to `out`.
    fn start_compositor(
        &self,
        layers: Vec<Layer>,
        out: Arc<FrameSlot>,
        width: u32,
        height: u32,
    ) -> Box<dyn SceneCompositor>;
}
