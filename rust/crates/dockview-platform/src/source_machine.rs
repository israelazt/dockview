//! The real implementation of the source ports: capture cards, screens and
//! windows, the audio engine, the loopback, the scene compositor and the test
//! pattern. `dockview-app` only sees the traits in `dockview_domain::ports`.

use std::sync::Arc;
use std::thread;

use dockview_domain::audio_status::AudioStatus;
use dockview_domain::device::{AudioDevice, DeviceLists};
use dockview_domain::frame::FrameSlot;
use dockview_domain::pcm::AudioTap;
use dockview_domain::ports::{
    AudioPlayback, CaptureHandle, Loopback, MicInput, SceneCompositor, ScreenHandle, SourceMachine,
};
use dockview_domain::sources::{
    CaptureOptions, CaptureStatus, CaptureTarget, Layer, MonitorInfo, ScreenOptions, ScreenStatus,
    WindowInfo,
};

use crate::audio::{default_capture_device, AudioEngine};
use crate::capture::VideoCapture;
use crate::compose::SceneCompositor as GpuCompositor;
use crate::device_watch::DeviceWatch;
use crate::screen::{list_monitors, list_windows, ScreenCapture};
use crate::tap_capture::{CaptureSource, TapCapture};
use crate::testsrc::TestSource;

/// The machine the app really runs on.
pub struct PlatformSources {
    devices: DeviceWatch,
}

impl PlatformSources {
    /// Lists the devices (waiting for the first list) and keeps watching them.
    pub fn new() -> Self {
        Self {
            devices: DeviceWatch::start(),
        }
    }
}

impl Default for PlatformSources {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs `work` on a helper thread. Media Foundation and WASAPI want an MTA
/// thread, while winit initialises the main thread as an STA.
fn on_helper_thread<T: Default + Send + 'static>(
    work: impl FnOnce() -> Option<T> + Send + 'static,
) -> T {
    thread::spawn(work)
        .join()
        .ok()
        .flatten()
        .unwrap_or_default()
}

impl SourceMachine for PlatformSources {
    fn start_capture(
        &self,
        symbolic_link: String,
        options: CaptureOptions,
        slot: Arc<FrameSlot>,
    ) -> Box<dyn CaptureHandle> {
        Box::new(VideoCapture::start(symbolic_link, options, slot))
    }

    fn start_screen(
        &self,
        target: CaptureTarget,
        options: ScreenOptions,
        slot: Arc<FrameSlot>,
    ) -> Box<dyn ScreenHandle> {
        Box::new(ScreenCapture::start(target, options, slot))
    }

    fn start_test_pattern(&self, slot: Arc<FrameSlot>, fps: u32) -> Box<dyn std::any::Any> {
        Box::new(TestSource::start(slot, 1920, 1080, fps))
    }

    fn list_monitors(&self) -> Vec<MonitorInfo> {
        list_monitors()
    }

    fn list_windows(&self) -> Vec<WindowInfo> {
        list_windows()
    }

    fn devices(&self) -> DeviceLists {
        self.devices.lists()
    }

    fn start_playback(
        &self,
        device_id: String,
        volume: f32,
        offset_ms: i32,
    ) -> Box<dyn AudioPlayback> {
        Box::new(AudioEngine::start(device_id, volume, offset_ms))
    }

    fn start_loopback(&self, source: Loopback, tap: Arc<AudioTap>) -> Box<dyn MicInput> {
        let source = match source {
            Loopback::System => CaptureSource::SystemLoopback,
            Loopback::Process(pid) => CaptureSource::ProcessLoopback(pid),
        };
        Box::new(TapCapture::start(source, 1.0, tap))
    }

    fn default_capture_device(&self) -> Option<AudioDevice> {
        on_helper_thread(|| Some(default_capture_device()))
    }

    fn start_compositor(
        &self,
        layers: Vec<Layer>,
        out: Arc<FrameSlot>,
        width: u32,
        height: u32,
    ) -> Box<dyn SceneCompositor> {
        Box::new(GpuCompositor::start(layers, out, width, height))
    }
}

impl CaptureHandle for VideoCapture {
    fn status(&self) -> CaptureStatus {
        VideoCapture::status(self)
    }
}

impl ScreenHandle for ScreenCapture {
    fn status(&self) -> ScreenStatus {
        ScreenCapture::status(self)
    }
}

impl AudioPlayback for AudioEngine {
    fn status(&self) -> AudioStatus {
        AudioEngine::status(self)
    }
    fn set_volume(&self, volume: f32) {
        AudioEngine::set_volume(self, volume);
    }
    fn set_offset_ms(&self, offset_ms: i32) {
        AudioEngine::set_offset_ms(self, offset_ms);
    }
    fn set_tap(&self, tap: Option<Arc<AudioTap>>) {
        AudioEngine::set_tap(self, tap);
    }
    fn buffered_ms(&self) -> u32 {
        AudioEngine::buffered_ms(self)
    }
}

impl SceneCompositor for GpuCompositor {
    fn set_layers(&self, layers: Vec<Layer>) {
        GpuCompositor::set_layers(self, layers);
    }
}
