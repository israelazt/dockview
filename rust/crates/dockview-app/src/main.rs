//! Dockview: minimal low-latency viewer for UVC capture cards.
//!
//! Keys: `F`/`F11` fullscreen · `Esc` leave fullscreen · `V` next video device
//! · `A` next audio device · `↑`/`↓` volume · `←`/`→` audio sync (±10 ms)
//! · `P` render preset.

#[cfg(not(windows))]
compile_error!("Dockview only supports Windows.");

mod cli;
#[allow(dead_code)] // some strings are used by the upcoming live controls
mod i18n;
mod render;
#[allow(dead_code)] // some tokens are used by the upcoming live controls
mod theme;
#[allow(dead_code)] // some helpers are used by the upcoming live controls
mod ui;

use std::ffi::c_void;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dockview_core::audio::{enumerate_audio_devices, AudioEngine, AudioStatus};
use dockview_core::capture::{CaptureOptions, CaptureStatus, VideoCapture};
use dockview_core::device::{match_audio_to_video, AudioDevice, VideoDevice};
use dockview_core::frame::FrameSlot;
use dockview_core::mf::{enumerate_video_devices, MfSession};
use dockview_core::settings::{self, AppSettings, RenderPreset, MAX_AUDIO_OFFSET_MS};
use dockview_core::testsrc::TestSource;
use windows::Win32::Foundation::HWND;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Fullscreen, Theme as WindowTheme, Window, WindowId};

use cli::Cli;
use i18n::Lang;
use render::{Filter, Renderer};
use theme::{Appearance, Theme};
use ui::gpu::UiGpu;
use ui::preview::{self, Sample};
use ui::Ui;

/// Delay before trying to reopen a capture device that went away.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);
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
        retry_at: Option<Instant>,
    },
    None,
}

struct App {
    slot: Arc<FrameSlot>,
    source: Source,
    video_devices: Vec<VideoDevice>,
    audio_devices: Vec<AudioDevice>,
    video_index: Option<usize>,
    audio_index: Option<usize>,
    audio: Option<AudioEngine>,
    settings: AppSettings,
    window: Option<Window>,
    renderer: Option<Renderer>,
    rendered: u64,
    last_stats: Instant,
    last_rendered: u64,
    video_size: (u32, u32),
    fullscreen: bool,
    cli: Cli,
    ui: Ui,
    ui_gpu: Option<UiGpu>,
    ui_scale: f32,
    system_appearance: Appearance,
    system_lang: Lang,
    exit_requested: bool,
}

fn filter_for(preset: RenderPreset) -> Filter {
    match preset {
        RenderPreset::Performance => Filter::Nearest,
        RenderPreset::Balanced | RenderPreset::Quality => Filter::Linear,
    }
}

fn appearance_of(theme: Option<WindowTheme>) -> Appearance {
    match theme {
        Some(WindowTheme::Light) => Appearance::Light,
        _ => Appearance::Dark,
    }
}

fn next_preset(preset: RenderPreset) -> RenderPreset {
    match preset {
        RenderPreset::Performance => RenderPreset::Balanced,
        RenderPreset::Balanced => RenderPreset::Quality,
        RenderPreset::Quality => RenderPreset::Performance,
    }
}

/// Index after `current` in a list of `len` items, wrapping around.
fn next_index(current: Option<usize>, len: usize) -> Option<usize> {
    (len > 0).then(|| current.map_or(0, |i| (i + 1) % len))
}

impl App {
    fn new(
        slot: Arc<FrameSlot>,
        settings: AppSettings,
        video_devices: Vec<VideoDevice>,
        audio_devices: Vec<AudioDevice>,
        cli: Cli,
    ) -> Self {
        let test_pattern = cli.test_pattern;
        let mut app = Self {
            slot,
            source: Source::None,
            video_devices,
            audio_devices,
            video_index: None,
            audio_index: None,
            audio: None,
            settings,
            window: None,
            renderer: None,
            rendered: 0,
            last_stats: Instant::now(),
            last_rendered: 0,
            video_size: (0, 0),
            fullscreen: false,
            cli,
            ui: Ui::new(),
            ui_gpu: None,
            ui_scale: 1.0,
            system_appearance: Appearance::Dark,
            system_lang: Lang::detect_system(),
            exit_requested: false,
        };

        if test_pattern {
            app.source = Source::Test(TestSource::start(Arc::clone(&app.slot), 1920, 1080, 30));
            return app;
        }

        // Restore the last devices, else the first video device and the audio
        // device whose name matches it.
        let video = app
            .video_devices
            .iter()
            .position(|d| {
                app.settings.last_video_device_symbolic_link.as_deref() == Some(&d.symbolic_link)
            })
            .or_else(|| next_index(None, app.video_devices.len()));
        let audio = app
            .audio_devices
            .iter()
            .position(|d| app.settings.last_audio_device_id.as_deref() == Some(&d.device_id))
            .or_else(|| app.match_audio(video))
            .or_else(|| next_index(None, app.audio_devices.len()));

        app.select_video(video);
        app.select_audio(audio);
        app
    }

    fn match_audio(&self, video: Option<usize>) -> Option<usize> {
        let matched = match_audio_to_video(
            video.and_then(|i| self.video_devices.get(i)),
            &self.audio_devices,
        )?;
        self.audio_devices.iter().position(|a| a == matched)
    }

    /// Starts capturing from the video device at `index` (or stops).
    fn select_video(&mut self, index: Option<usize>) {
        self.video_index = index;
        self.video_size = (0, 0);
        // Drop the old capture first so it releases the device.
        self.source = Source::None;
        let Some(device) = index.and_then(|i| self.video_devices.get(i)) else {
            return;
        };
        self.settings.last_video_device_symbolic_link = Some(device.symbolic_link.clone());
        self.source = Source::Capture {
            capture: self.start_capture(&device.symbolic_link.clone()),
            retry_at: None,
        };
    }

    fn start_capture(&self, symbolic_link: &str) -> VideoCapture {
        VideoCapture::start(
            symbolic_link.to_owned(),
            CaptureOptions::default(),
            Arc::clone(&self.slot),
        )
    }

    /// Starts the audio pipeline for the audio device at `index` (or stops).
    fn select_audio(&mut self, index: Option<usize>) {
        self.audio_index = index;
        self.audio = None;
        let Some(device) = index.and_then(|i| self.audio_devices.get(i)) else {
            return;
        };
        self.settings.last_audio_device_id = Some(device.device_id.clone());
        self.audio = Some(AudioEngine::start(
            device.device_id.clone(),
            self.settings.volume,
            self.settings.audio_offset_ms,
        ));
    }

    fn change_volume(&mut self, delta: f32) {
        self.settings.volume = (self.settings.volume + delta).clamp(0.0, 1.0);
        // Snap away accumulated float error so 5% steps stay exact.
        self.settings.volume = (self.settings.volume * 100.0).round() / 100.0;
        if let Some(audio) = &self.audio {
            audio.set_volume(self.settings.volume);
        }
        self.save_settings();
    }

    fn change_offset(&mut self, delta_ms: i32) {
        self.settings.audio_offset_ms = (self.settings.audio_offset_ms + delta_ms)
            .clamp(-MAX_AUDIO_OFFSET_MS, MAX_AUDIO_OFFSET_MS);
        if let Some(audio) = &self.audio {
            audio.set_offset_ms(self.settings.audio_offset_ms);
        }
        self.save_settings();
    }

    fn cycle_preset(&mut self) {
        self.settings.render_preset = next_preset(self.settings.render_preset);
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_filter(filter_for(self.settings.render_preset));
        }
        self.save_settings();
    }

    fn save_settings(&mut self) {
        self.settings.was_fullscreen = self.fullscreen;
        settings::save(&self.settings);
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

        let preview_mode = self.cli.ui_preview;
        let theme = Theme::for_appearance(self.cli.theme.resolve(self.system_appearance));
        let lang = self.cli.lang.resolve(self.system_lang);
        let scale = self.ui_scale;
        let capture = self.cli.screenshot.is_some() && self.rendered >= SCREENSHOT_AFTER_FRAMES;
        let (ui, ui_gpu) = (&mut self.ui, &mut self.ui_gpu);

        let result = renderer.draw(
            &mut |_device, context, size| {
                let Some(gpu) = ui_gpu.as_mut() else {
                    return;
                };
                if preview_mode {
                    ui.begin(size, scale);
                    preview::draw(ui, &theme, lang, &Sample::default());
                    if let Err(e) = gpu.render(context, ui, size) {
                        eprintln!("ui draw failed: {e}");
                    }
                }
            },
            capture,
        );
        match result {
            Ok(Some(shot)) => self.save_screenshot(&shot),
            Ok(None) => {}
            Err(e) => eprintln!("present failed: {e}"),
        }
    }

    fn save_screenshot(&mut self, shot: &render::Screenshot) {
        self.exit_requested = true;
        let Some(path) = &self.cli.screenshot else {
            return;
        };
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        let result = std::fs::File::create(path)
            .map_err(|e| e.to_string())
            .and_then(|file| {
                let mut encoder =
                    png::Encoder::new(std::io::BufWriter::new(file), shot.width, shot.height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .and_then(|mut w| w.write_image_data(&shot.rgba))
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(()) => println!("saved {}", path.display()),
            Err(e) => eprintln!("could not save {}: {e}", path.display()),
        }
    }

    fn set_fullscreen(&mut self, on: bool) {
        self.fullscreen = on;
        if let Some(window) = &self.window {
            window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
        }
        self.save_settings();
    }

    /// Restarts the capture thread if the device was lost.
    fn maintain_source(&mut self) {
        let Some(device) = self.video_index.and_then(|i| self.video_devices.get(i)) else {
            return;
        };
        let link = device.symbolic_link.clone();
        let Source::Capture { capture, retry_at } = &self.source else {
            return;
        };
        match capture.status() {
            CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_) => {
                let due = retry_at.unwrap_or_else(|| Instant::now() + RECONNECT_DELAY);
                if Instant::now() >= due {
                    let capture = self.start_capture(&link);
                    self.source = Source::Capture {
                        capture,
                        retry_at: None,
                    };
                } else if let Source::Capture { retry_at, .. } = &mut self.source {
                    *retry_at = Some(due);
                }
            }
            _ => {
                if let Source::Capture { retry_at, .. } = &mut self.source {
                    *retry_at = None;
                }
            }
        }
    }

    fn video_status(&self) -> String {
        let name = self
            .video_index
            .and_then(|i| self.video_devices.get(i))
            .map_or("no video device", |d| d.friendly_name.as_str());
        match &self.source {
            Source::Test(_) => "test pattern".to_string(),
            Source::None => name.to_string(),
            Source::Capture { capture, .. } => match capture.status() {
                CaptureStatus::Running { .. } | CaptureStatus::Stopped => name.to_string(),
                CaptureStatus::Starting => format!("{name} (connecting…)"),
                CaptureStatus::Disconnected { .. } => format!("{name} (disconnected, retrying…)"),
                CaptureStatus::Failed(e) => format!("{name} (error: {e})"),
            },
        }
    }

    fn audio_status(&self) -> String {
        let name = self
            .audio_index
            .and_then(|i| self.audio_devices.get(i))
            .map_or("none", |d| d.friendly_name.as_str());
        match self.audio.as_ref().map(AudioEngine::status) {
            Some(AudioStatus::Disconnected(_)) => format!("{name} (lost)"),
            Some(AudioStatus::Failed(e)) => format!("{name} (error: {e})"),
            _ => name.to_string(),
        }
    }

    fn update_title(&mut self) {
        if self.cli.ui_preview || self.last_stats.elapsed() < Duration::from_secs(1) {
            return;
        }
        let secs = self.last_stats.elapsed().as_secs_f64();
        let fps = (self.rendered - self.last_rendered) as f64 / secs;
        self.last_rendered = self.rendered;
        self.last_stats = Instant::now();

        let (w, h) = self.video_size;
        let video = if w > 0 {
            format!("{} · {w}×{h} · {fps:.0} fps", self.video_status())
        } else {
            self.video_status()
        };
        let title = format!(
            "Dockview · {video} · 🔊 {} {:.0}% · sync {:+} ms",
            self.audio_status(),
            self.settings.volume * 100.0,
            self.settings.audio_offset_ms,
        );
        if let Some(window) = &self.window {
            window.set_title(&title);
        }
    }

    fn on_key(&mut self, key: &Key) {
        match key {
            Key::Named(NamedKey::F11) => self.set_fullscreen(!self.fullscreen),
            Key::Named(NamedKey::Escape) if self.fullscreen => self.set_fullscreen(false),
            Key::Named(NamedKey::ArrowUp) => self.change_volume(VOLUME_STEP),
            Key::Named(NamedKey::ArrowDown) => self.change_volume(-VOLUME_STEP),
            Key::Named(NamedKey::ArrowLeft) => self.change_offset(-OFFSET_STEP_MS),
            Key::Named(NamedKey::ArrowRight) => self.change_offset(OFFSET_STEP_MS),
            Key::Character(c) => match c.to_ascii_lowercase().as_str() {
                "f" => self.set_fullscreen(!self.fullscreen),
                "p" => self.cycle_preset(),
                "v" => {
                    let next = next_index(self.video_index, self.video_devices.len());
                    self.select_video(next);
                    // Follow the new video device with its matching audio.
                    if let Some(audio) = self.match_audio(next) {
                        self.select_audio(Some(audio));
                    }
                    self.save_settings();
                }
                "a" => {
                    let next = next_index(self.audio_index, self.audio_devices.len());
                    self.select_audio(next);
                    self.save_settings();
                }
                _ => {}
            },
            _ => {}
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = if self.cli.ui_preview {
            "Dockview — UI preview"
        } else {
            "Dockview"
        };
        let mut attributes = Window::default_attributes().with_title(title);
        attributes = match self.cli.scale {
            Some(scale) => attributes.with_inner_size(PhysicalSize::new(
                (1280.0 * scale).round(),
                (720.0 * scale).round(),
            )),
            None => attributes.with_inner_size(LogicalSize::new(1280.0, 720.0)),
        };
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
                renderer.set_filter(filter_for(self.settings.render_preset));
                match UiGpu::new(renderer.device()) {
                    Ok(gpu) => self.ui_gpu = Some(gpu),
                    Err(e) => eprintln!("could not initialise the interface renderer: {e}"),
                }
                self.renderer = Some(renderer);
            }
            Err(e) => {
                eprintln!("could not initialise Direct3D: {e}");
                event_loop.exit();
                return;
            }
        }
        self.ui_scale = self.cli.scale.unwrap_or(window.scale_factor() as f32);
        self.system_appearance = appearance_of(window.theme());
        self.window = Some(window);
        if self.settings.was_fullscreen {
            self.set_fullscreen(true);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.save_settings();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(e) = renderer.resize(size.width, size.height) {
                        eprintln!("resize failed: {e}");
                    }
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::ThemeChanged(theme) => self.system_appearance = appearance_of(Some(theme)),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ui_scale = self.cli.scale.unwrap_or(scale_factor as f32);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.on_key(&event.logical_key);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.exit_requested {
            event_loop.exit();
            return;
        }
        self.maintain_source();
        self.update_title();
        if self.slot.wait_for_frame(FRAME_WAIT) {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
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

fn main() {
    let slot = Arc::new(FrameSlot::new());
    let cli = match Cli::parse(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(e) => {
            eprintln!("dockview: {e}");
            std::process::exit(2);
        }
    };
    let test_pattern = cli.test_pattern;

    let (video_devices, audio_devices) = if test_pattern {
        (Vec::new(), Vec::new())
    } else {
        (
            on_helper_thread(|| {
                let session = MfSession::start().ok()?;
                enumerate_video_devices(&session).ok()
            }),
            on_helper_thread(|| enumerate_audio_devices().ok()),
        )
    };

    let event_loop = match EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("could not create event loop: {e}");
            return;
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(slot, settings::load(), video_devices, audio_devices, cli);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("event loop error: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_index_wraps_and_handles_empty_lists() {
        assert_eq!(next_index(None, 0), None);
        assert_eq!(next_index(None, 3), Some(0));
        assert_eq!(next_index(Some(0), 3), Some(1));
        assert_eq!(next_index(Some(2), 3), Some(0));
    }

    #[test]
    fn presets_cycle_through_all_three() {
        let start = RenderPreset::Performance;
        let cycled = next_preset(next_preset(next_preset(start)));
        assert_eq!(cycled, start);
        assert_ne!(next_preset(start), start);
    }

    #[test]
    fn only_performance_uses_nearest_filtering() {
        assert_eq!(filter_for(RenderPreset::Performance), Filter::Nearest);
        assert_eq!(filter_for(RenderPreset::Balanced), Filter::Linear);
        assert_eq!(filter_for(RenderPreset::Quality), Filter::Linear);
    }
}
