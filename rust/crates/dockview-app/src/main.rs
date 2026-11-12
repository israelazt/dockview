//! Dockview: minimal low-latency viewer for UVC capture cards.

#[cfg(not(windows))]
compile_error!("Dockview only supports Windows.");

mod render;

use std::ffi::c_void;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dockview_core::capture::{CaptureOptions, CaptureStatus, VideoCapture};
use dockview_core::device::VideoDevice;
use dockview_core::frame::FrameSlot;
use dockview_core::mf::{enumerate_video_devices, MfSession};
use dockview_core::settings::{self, RenderPreset};
use dockview_core::testsrc::TestSource;
use windows::Win32::Foundation::HWND;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Fullscreen, Window, WindowId};

use render::{Filter, Renderer};

/// Delay before trying to reopen a capture device that went away.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
/// How long the event loop waits for a new frame before checking events.
const FRAME_WAIT: Duration = Duration::from_millis(4);

enum Source {
    /// The handle only keeps the generator thread alive.
    Test(#[allow(dead_code)] TestSource),
    Capture {
        device: VideoDevice,
        capture: VideoCapture,
        retry_at: Option<Instant>,
    },
    None,
}

struct App {
    slot: Arc<FrameSlot>,
    source: Source,
    window: Option<Window>,
    renderer: Option<Renderer>,
    rendered: u64,
    last_stats: Instant,
    last_rendered: u64,
    video_size: (u32, u32),
    fullscreen: bool,
    filter: Filter,
}

impl App {
    fn new(slot: Arc<FrameSlot>, source: Source, filter: Filter) -> Self {
        Self {
            slot,
            source,
            window: None,
            renderer: None,
            rendered: 0,
            last_stats: Instant::now(),
            last_rendered: 0,
            video_size: (0, 0),
            fullscreen: false,
            filter,
        }
    }

    fn render(&mut self) {
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // Wait for a free present slot *before* picking the frame so we always
        // show the freshest one.
        renderer.wait_for_present_slot(100);
        if let Some(frame) = self.slot.take_latest() {
            self.video_size = (frame.width, frame.height);
            if let Err(e) = renderer.upload(&frame) {
                eprintln!("upload failed: {e}");
            }
            self.slot.recycle(frame);
            self.rendered += 1;
        }
        if let Err(e) = renderer.draw() {
            eprintln!("present failed: {e}");
        }
    }

    fn toggle_fullscreen(&mut self) {
        self.set_fullscreen(!self.fullscreen);
    }

    fn set_fullscreen(&mut self, on: bool) {
        self.fullscreen = on;
        if let Some(window) = &self.window {
            window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
        }
    }

    /// Restarts the capture thread if the device was lost.
    fn maintain_source(&mut self) {
        let Source::Capture {
            device,
            capture,
            retry_at,
        } = &mut self.source
        else {
            return;
        };
        match capture.status() {
            CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_) => {
                let due = *retry_at.get_or_insert_with(|| Instant::now() + RECONNECT_DELAY);
                if Instant::now() >= due {
                    *capture = VideoCapture::start(
                        device.symbolic_link.clone(),
                        CaptureOptions::default(),
                        Arc::clone(&self.slot),
                    );
                    *retry_at = None;
                }
            }
            _ => *retry_at = None,
        }
    }

    fn update_title(&mut self) {
        if self.last_stats.elapsed() < Duration::from_secs(1) {
            return;
        }
        let secs = self.last_stats.elapsed().as_secs_f64();
        let fps = (self.rendered - self.last_rendered) as f64 / secs;
        self.last_rendered = self.rendered;
        self.last_stats = Instant::now();

        let detail = match &self.source {
            Source::Test(_) => "test pattern".to_string(),
            Source::None => "no capture device found".to_string(),
            Source::Capture {
                device, capture, ..
            } => match capture.status() {
                CaptureStatus::Running { .. } => device.friendly_name.clone(),
                CaptureStatus::Starting => format!("{} (connecting…)", device.friendly_name),
                CaptureStatus::Disconnected { .. } => {
                    format!("{} (disconnected, retrying…)", device.friendly_name)
                }
                CaptureStatus::Failed(e) => format!("{} (error: {e})", device.friendly_name),
                CaptureStatus::Stopped => device.friendly_name.clone(),
            },
        };
        let (w, h) = self.video_size;
        let title = if w > 0 {
            format!(
                "Dockview · {detail} · {w}×{h} · {fps:.0} fps · dropped {}",
                self.slot.dropped()
            )
        } else {
            format!("Dockview · {detail}")
        };
        if let Some(window) = &self.window {
            window.set_title(&title);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Dockview")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = match event_loop.create_window(attributes) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("could not create window: {e}");
                event_loop.exit();
                return;
            }
        };

        let hwnd = match window.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => HWND(h.hwnd.get() as *mut c_void),
            _ => {
                eprintln!("window has no Win32 handle");
                event_loop.exit();
                return;
            }
        };
        let size = window.inner_size();
        match Renderer::new(hwnd, size.width, size.height) {
            Ok(mut renderer) => {
                renderer.set_filter(self.filter);
                self.renderer = Some(renderer);
            }
            Err(e) => {
                eprintln!("could not initialise Direct3D: {e}");
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(e) = renderer.resize(size.width, size.height) {
                        eprintln!("resize failed: {e}");
                    }
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key {
                    Key::Named(NamedKey::F11) => self.toggle_fullscreen(),
                    Key::Named(NamedKey::Escape) if self.fullscreen => self.set_fullscreen(false),
                    Key::Character(c) if c.eq_ignore_ascii_case("f") => self.toggle_fullscreen(),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.maintain_source();
        self.update_title();
        if self.slot.wait_for_frame(FRAME_WAIT) {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

/// Lists capture devices on a helper thread: Media Foundation wants an MTA
/// thread, while winit initialises the main thread as an STA.
fn list_devices() -> Vec<VideoDevice> {
    thread::spawn(|| {
        let session = MfSession::start().ok()?;
        enumerate_video_devices(&session).ok()
    })
    .join()
    .ok()
    .flatten()
    .unwrap_or_default()
}

fn main() {
    let slot = Arc::new(FrameSlot::new());
    let use_test_pattern = std::env::args().any(|a| a == "--test-pattern");

    let settings = settings::load();
    let filter = match settings.render_preset {
        RenderPreset::Performance => Filter::Nearest,
        RenderPreset::Balanced | RenderPreset::Quality => Filter::Linear,
    };

    let source = if use_test_pattern {
        Source::Test(TestSource::start(Arc::clone(&slot), 1920, 1080, 30))
    } else {
        let devices = list_devices();
        let chosen = devices
            .iter()
            .find(|d| settings.last_video_device_symbolic_link.as_deref() == Some(&d.symbolic_link))
            .or_else(|| devices.first())
            .cloned();
        match chosen {
            Some(device) => {
                let capture = VideoCapture::start(
                    device.symbolic_link.clone(),
                    CaptureOptions::default(),
                    Arc::clone(&slot),
                );
                Source::Capture {
                    device,
                    capture,
                    retry_at: None,
                }
            }
            None => Source::None,
        }
    };

    let event_loop = match EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("could not create event loop: {e}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(slot, source, filter);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("event loop error: {e}");
    }
}
