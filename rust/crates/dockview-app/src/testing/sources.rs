//! A fake `SourceMachine`: it opens no device and captures nothing. Every
//! source it is asked to start is written down in [`SourcesWorld`], where the
//! test can change what it reports (a cable pulled, a window closed) and see
//! whether the code under test let go of it (`alive`).

use std::sync::{Arc, Mutex};

use dockview_domain::audio_status::AudioStatus;
use dockview_domain::device::AudioDevice;
use dockview_domain::frame::{FrameSlot, PixelFormat};
use dockview_domain::pcm::AudioTap;
use dockview_domain::ports::{
    AudioPlayback, CaptureHandle, Loopback, MicInput, SceneCompositor, ScreenHandle, SourceMachine,
};
use dockview_domain::sources::{
    CaptureOptions, CaptureStatus, CaptureTarget, Layer, MonitorInfo, ScreenOptions, ScreenStatus,
    WindowInfo,
};
use dockview_domain::stream::StreamInfo;

/// A value a fake handle shares with the test.
pub type Live<T> = Arc<Mutex<T>>;

/// Marks the shared state of a handle as gone when the handle is dropped.
pub trait Dies {
    fn die(&mut self);
}

/// The handle given to the code under test. Dropping it closes the source.
pub struct Handle<T: Dies>(Live<T>);

impl<T: Dies> Drop for Handle<T> {
    fn drop(&mut self) {
        self.0.lock().unwrap().die();
    }
}

pub struct CaptureState {
    pub link: String,
    pub options: CaptureOptions,
    pub status: CaptureStatus,
    pub alive: bool,
}

pub struct ScreenState {
    pub target: CaptureTarget,
    pub status: ScreenStatus,
    pub alive: bool,
}

pub struct PlaybackState {
    pub device_id: String,
    pub volume: f32,
    pub offset_ms: i32,
    pub status: AudioStatus,
    /// Whether the recorder is being fed (`set_tap(Some(..))` was the last call).
    pub tapped: bool,
    pub alive: bool,
}

pub struct LoopbackState {
    pub source: Loopback,
    pub alive: bool,
}

pub struct CompositorState {
    pub layers: usize,
    pub alive: bool,
}

macro_rules! dies {
    ($($t:ty),*) => {$(impl Dies for $t { fn die(&mut self) { self.alive = false; } })*};
}
dies!(
    CaptureState,
    ScreenState,
    PlaybackState,
    LoopbackState,
    CompositorState
);

/// What a capture card reports once it is running: a 1080p60 YUY2 mode.
pub fn running_capture() -> CaptureStatus {
    CaptureStatus::Running {
        native: StreamInfo {
            type_index: 0,
            format_name: "YUY2".into(),
            width: 1920,
            height: 1080,
            fps: 60,
            is_compressed: false,
        },
        output: PixelFormat::Yuy2,
        has_compressed: false,
    }
}

/// Everything the fake machine was asked to start, and what it should list.
#[derive(Default)]
pub struct SourcesWorld {
    pub monitors: Vec<MonitorInfo>,
    pub windows: Vec<WindowInfo>,
    pub default_audio: Option<AudioDevice>,
    pub captures: Vec<Live<CaptureState>>,
    pub screens: Vec<Live<ScreenState>>,
    pub playbacks: Vec<Live<PlaybackState>>,
    pub loopbacks: Vec<Live<LoopbackState>>,
    pub compositors: Vec<Live<CompositorState>>,
    /// The frame rate of each test pattern started.
    pub test_patterns: Vec<u32>,
}

impl SourcesWorld {
    /// The capture started last.
    pub fn last_capture(&self) -> Live<CaptureState> {
        Arc::clone(self.captures.last().expect("no capture was started"))
    }

    /// The playback started last.
    pub fn last_playback(&self) -> Live<PlaybackState> {
        Arc::clone(self.playbacks.last().expect("no playback was started"))
    }

    /// The screen capture started last.
    pub fn last_screen(&self) -> Live<ScreenState> {
        Arc::clone(self.screens.last().expect("no screen was started"))
    }

    /// How many captures are open now.
    pub fn open_captures(&self) -> usize {
        self.captures
            .iter()
            .filter(|c| c.lock().unwrap().alive)
            .count()
    }

    /// How many playbacks are open now.
    pub fn open_playbacks(&self) -> usize {
        self.playbacks
            .iter()
            .filter(|p| p.lock().unwrap().alive)
            .count()
    }
}

pub type SharedSources = Live<SourcesWorld>;

pub struct FakeSources(pub SharedSources);

impl FakeSources {
    pub fn new() -> (Self, SharedSources) {
        let world = SharedSources::default();
        (Self(Arc::clone(&world)), world)
    }
}

impl CaptureHandle for Handle<CaptureState> {
    fn status(&self) -> CaptureStatus {
        self.0.lock().unwrap().status.clone()
    }
}

impl ScreenHandle for Handle<ScreenState> {
    fn status(&self) -> ScreenStatus {
        self.0.lock().unwrap().status.clone()
    }
}

impl AudioPlayback for Handle<PlaybackState> {
    fn status(&self) -> AudioStatus {
        self.0.lock().unwrap().status.clone()
    }
    fn set_volume(&self, volume: f32) {
        self.0.lock().unwrap().volume = volume;
    }
    fn set_offset_ms(&self, offset_ms: i32) {
        self.0.lock().unwrap().offset_ms = offset_ms;
    }
    fn set_tap(&self, tap: Option<Arc<AudioTap>>) {
        self.0.lock().unwrap().tapped = tap.is_some();
    }
    fn buffered_ms(&self) -> u32 {
        50
    }
}

impl MicInput for Handle<LoopbackState> {
    fn status(&self) -> AudioStatus {
        AudioStatus::Running
    }
    fn level(&self) -> f32 {
        0.0
    }
}

impl SceneCompositor for Handle<CompositorState> {
    fn set_layers(&self, layers: Vec<Layer>) {
        self.0.lock().unwrap().layers = layers.len();
    }
}

impl SourceMachine for FakeSources {
    fn start_capture(
        &self,
        symbolic_link: String,
        options: CaptureOptions,
        _: Arc<FrameSlot>,
    ) -> Box<dyn CaptureHandle> {
        let state = Live::new(Mutex::new(CaptureState {
            link: symbolic_link,
            options,
            status: running_capture(),
            alive: true,
        }));
        self.0.lock().unwrap().captures.push(Arc::clone(&state));
        Box::new(Handle(state))
    }

    fn start_screen(
        &self,
        target: CaptureTarget,
        _: ScreenOptions,
        _: Arc<FrameSlot>,
    ) -> Box<dyn ScreenHandle> {
        let state = Live::new(Mutex::new(ScreenState {
            target,
            status: ScreenStatus::Running {
                width: 1920,
                height: 1080,
            },
            alive: true,
        }));
        self.0.lock().unwrap().screens.push(Arc::clone(&state));
        Box::new(Handle(state))
    }

    fn start_test_pattern(&self, _: Arc<FrameSlot>, fps: u32) -> Box<dyn std::any::Any> {
        self.0.lock().unwrap().test_patterns.push(fps);
        Box::new(())
    }

    fn list_monitors(&self) -> Vec<MonitorInfo> {
        self.0.lock().unwrap().monitors.clone()
    }

    fn list_windows(&self) -> Vec<WindowInfo> {
        self.0.lock().unwrap().windows.clone()
    }

    fn start_playback(
        &self,
        device_id: String,
        volume: f32,
        offset_ms: i32,
    ) -> Box<dyn AudioPlayback> {
        let state = Live::new(Mutex::new(PlaybackState {
            device_id,
            volume,
            offset_ms,
            status: AudioStatus::Running,
            tapped: false,
            alive: true,
        }));
        self.0.lock().unwrap().playbacks.push(Arc::clone(&state));
        Box::new(Handle(state))
    }

    fn start_loopback(&self, source: Loopback, _: Arc<AudioTap>) -> Box<dyn MicInput> {
        let state = Live::new(Mutex::new(LoopbackState {
            source,
            alive: true,
        }));
        self.0.lock().unwrap().loopbacks.push(Arc::clone(&state));
        Box::new(Handle(state))
    }

    fn default_capture_device(&self) -> Option<AudioDevice> {
        self.0.lock().unwrap().default_audio.clone()
    }

    fn start_compositor(
        &self,
        layers: Vec<Layer>,
        _: Arc<FrameSlot>,
        _: u32,
        _: u32,
    ) -> Box<dyn SceneCompositor> {
        let state = Live::new(Mutex::new(CompositorState {
            layers: layers.len(),
            alive: true,
        }));
        self.0.lock().unwrap().compositors.push(Arc::clone(&state));
        Box::new(Handle(state))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_started_capture_is_written_down_and_reports_what_the_test_sets() {
        let (machine, world) = FakeSources::new();
        let handle = machine.start_capture(
            "link-1".into(),
            CaptureOptions::default(),
            Arc::new(FrameSlot::new()),
        );
        assert!(matches!(handle.status(), CaptureStatus::Running { .. }));
        assert_eq!(
            world.lock().unwrap().last_capture().lock().unwrap().link,
            "link-1"
        );

        world.lock().unwrap().last_capture().lock().unwrap().status = CaptureStatus::Disconnected {
            reason: "unplugged".into(),
        };
        assert!(matches!(
            handle.status(),
            CaptureStatus::Disconnected { .. }
        ));
    }

    #[test]
    fn dropping_a_handle_closes_the_source() {
        let (machine, world) = FakeSources::new();
        let handle = machine.start_playback("mic".into(), 0.5, 20);
        assert_eq!(world.lock().unwrap().open_playbacks(), 1);
        handle.set_volume(0.8);
        assert_eq!(
            world.lock().unwrap().last_playback().lock().unwrap().volume,
            0.8
        );
        drop(handle);
        assert_eq!(world.lock().unwrap().open_playbacks(), 0);
    }
}
