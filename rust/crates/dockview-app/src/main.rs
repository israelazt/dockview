//! Dockview: minimal low-latency viewer for UVC capture cards.
//!
//! Keys: `F`/`F11` fullscreen · `Esc` leave fullscreen · `V` next video device
//! · `A` next audio device · `↑`/`↓` volume · `←`/`→` audio sync (±10 ms)
//! · `P` render preset.

#[cfg(not(windows))]
compile_error!("Dockview only supports Windows.");

mod cli;
mod clips;
mod hotkey;
#[allow(dead_code)] // some strings are used by the upcoming live controls
mod i18n;
mod perf;
mod render;
mod stats_log;
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
use dockview_core::diagnostics::{
    diagnose, switch_helped, FallbackWatch, Finding, Snapshot, VideoState,
};
use dockview_core::frame::FrameSlot;
use dockview_core::mf::{enumerate_video_devices, MfSession};
use dockview_core::mic::MicCapture;
use dockview_core::recorder::{RecorderStatus, ReplayRecorder, ReplaySettings, SaveHandle};
use dockview_core::recording::{disk_free_bytes, EndReason, RecordingState, DISK_START_MIN_BYTES};
use dockview_core::settings::{
    self, AppSettings, LangMode, RenderPreset, ThemeMode, VideoModePref, MAX_AUDIO_OFFSET_MS,
};
use dockview_core::stream::StreamInfo;
use dockview_core::testsrc::TestSource;
use windows::Win32::Foundation::HWND;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{CursorIcon, Fullscreen, Theme as WindowTheme, Window, WindowId};

use cli::Cli;
use i18n::{Lang, LangModeExt};
use perf::ProcessSampler;
use render::{Filter, Renderer};
use theme::{Appearance, Theme, ThemeModeExt};
use ui::controls::{Action, Controls, MicState, MicView, Model, RecState, RecView, StatusKind};
use ui::gpu::UiGpu;
use ui::input::Pointer;
use ui::stats::{Stats, StatsPanel};
use ui::toast::{ToastHost, ToastKind};
use ui::Ui;

/// Seconds after an automatic mode switch before its effect is judged.
const SWITCH_CHECK_SECS: u32 = 7;
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
    controls: Controls,
    pointer: Pointer,
    last_frame: Instant,
    fps: f32,
    needs_redraw: bool,
    cursor_pointer: bool,
    cursor_hidden: bool,
    animations: bool,
    stats_open: bool,
    stats_panel: StatsPanel,
    stats: Stats,
    sampler: ProcessSampler,
    last_published: u64,
    last_dropped: u64,
    latency_sum_ms: f64,
    latency_count: u32,
    stats_updates: u32,
    stats_log: Option<stats_log::StatsLog>,
    fallback: FallbackWatch,
    /// The app switched to a compressed mode by itself this session.
    auto_switched: bool,
    /// A switch was already tried this session; never try twice.
    fallback_tried: bool,
    /// After a switch: seconds until its effect is judged, and the rate before.
    switch_check: Option<(u32, f32)>,
    /// The instant-replay buffer; `Some` while it is on.
    recorder: Option<ReplayRecorder>,
    /// A clip being written in the background.
    saving: Option<SaveHandle>,
    hotkey: Option<hotkey::Hotkey>,
    toasts: ToastHost,
    /// Installed RAM, which caps the replay length.
    total_ram: u64,
    /// The microphone; `Some` only while the user has it on.
    mic: Option<MicCapture>,
    /// Recording was asked for while the buffer was still starting.
    record_when_ready: bool,
    /// The newest clip or recording saved this session, to show in Explorer.
    last_saved: Option<std::path::PathBuf>,
    /// The low-disk warning was already shown for this recording.
    low_disk_warned: bool,
    /// Smoothed loudness for the meter.
    mic_level: f32,
    mic_level_at: Instant,
}

/// Whether Windows allows UI animations ("Show animations in Windows").
fn system_animations_enabled() -> bool {
    use windows::core::BOOL;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut enabled = BOOL(1);
    // SAFETY: the out-pointer refers to a live BOOL, as SPI_GETCLIENTAREAANIMATION requires.
    let result = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    result.map_or(true, |()| enabled.as_bool())
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
        let preview = cli.ui_preview;
        let show_stats = cli.show_stats;
        let stats_log = cli
            .log_stats
            .as_deref()
            .and_then(|path| stats_log::StatsLog::create(path).ok());
        // Screenshots must show the settled state, not a mid-transition frame.
        let animations = cli.screenshot.is_none() && system_animations_enabled();
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
            controls: Controls::new(preview),
            pointer: Pointer::default(),
            last_frame: Instant::now(),
            fps: 0.0,
            needs_redraw: true,
            cursor_pointer: false,
            cursor_hidden: false,
            animations,
            stats_open: show_stats,
            stats_panel: StatsPanel::new(),
            stats: Stats::default(),
            sampler: ProcessSampler::new(),
            last_published: 0,
            last_dropped: 0,
            latency_sum_ms: 0.0,
            latency_count: 0,
            stats_updates: 0,
            stats_log,
            fallback: FallbackWatch::new(),
            auto_switched: false,
            fallback_tried: false,
            switch_check: None,
            recorder: None,
            saving: None,
            hotkey: None,
            toasts: ToastHost::new(),
            total_ram: perf::total_ram_bytes(),
            mic: None,
            record_when_ready: false,
            last_saved: None,
            low_disk_warned: false,
            mic_level: 0.0,
            mic_level_at: Instant::now(),
        };

        app.controls.set_animations(animations);

        if test_pattern {
            app.source = Source::Test(TestSource::start(Arc::clone(&app.slot), 1920, 1080, 30));
            if preview {
                app.install_preview_devices();
            }
            if app.cli.open_settings {
                app.controls.toggle_settings();
            }
            if app.cli.open_replay {
                app.controls.toggle_replay();
            }
            app.start_replay_services();
            if app.cli.mic {
                app.set_mic(true);
            }
            if app.cli.record {
                app.toggle_record();
            }
            return app;
        }

        // Restore the last devices, else the first video device and the audio
        // device whose name matches it.
        let by_name = app
            .cli
            .video
            .as_deref()
            .map(str::to_lowercase)
            .and_then(|wanted| {
                app.video_devices
                    .iter()
                    .position(|d| d.friendly_name.to_lowercase().contains(&wanted))
            });
        let video = by_name
            .or_else(|| {
                app.video_devices.iter().position(|d| {
                    app.settings.last_video_device_symbolic_link.as_deref()
                        == Some(&d.symbolic_link)
                })
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
        app.start_replay_services();
        if app.cli.mic {
            app.set_mic(true);
        }
        if app.cli.record {
            app.toggle_record();
        }
        app
    }

    /// Registers the hotkey and starts the replay buffer if it is enabled.
    fn start_replay_services(&mut self) {
        if self.cli.ui_preview {
            return;
        }
        self.clamp_replay_length();
        let hotkey = hotkey::Hotkey::register();
        if !hotkey.is_registered() {
            eprintln!(
                "replay hotkey {} is taken by another program",
                hotkey::LABEL
            );
        }
        if !hotkey.record_registered() {
            eprintln!(
                "record hotkey {} is taken by another program",
                hotkey::RECORD_LABEL
            );
        }
        self.hotkey = Some(hotkey);
        self.sync_replay();
    }

    /// Makes the recorder match `settings.replay_enabled`.
    fn sync_replay(&mut self) {
        if self.settings.replay_enabled == self.recorder.is_some() {
            return;
        }
        // The microphone rides on the recorder: it goes when the buffer goes.
        self.mic = None;
        if self.settings.replay_enabled {
            let recorder = ReplayRecorder::start(
                Arc::clone(&self.slot),
                ReplaySettings {
                    // A little extra so a clip of the chosen length always fits
                    // (it starts at the keyframe before the cut).
                    keep_seconds: f64::from(
                        self.settings.replay_seconds + settings::REPLAY_MARGIN_SECONDS,
                    ),
                    bitrate_mbps: self.settings.replay_bitrate_mbps,
                    fps: 60,
                },
            );
            if let Some(audio) = &self.audio {
                audio.set_tap(Some(recorder.audio_tap()));
            }
            self.recorder = Some(recorder);
        } else {
            if let Some(audio) = &self.audio {
                audio.set_tap(None);
            }
            self.recorder = None;
        }
    }

    /// Names for the microphone list: the Windows default, then every input.
    fn mic_device_names(&self, lang: Lang) -> Vec<String> {
        std::iter::once(i18n::tr(lang, i18n::Key::MicDefault).to_owned())
            .chain(self.audio_devices.iter().map(|d| d.friendly_name.clone()))
            .collect()
    }

    /// Index in [`Self::mic_device_names`] of the chosen microphone.
    fn mic_selected(&self) -> usize {
        self.settings
            .last_mic_device_id
            .as_deref()
            .and_then(|id| self.audio_devices.iter().position(|d| d.device_id == id))
            .map_or(0, |i| i + 1)
    }

    fn rec_view(&self) -> RecView {
        if self.record_when_ready {
            return RecView {
                state: RecState::Starting,
                seconds: 0.0,
            };
        }
        let Some(recorder) = &self.recorder else {
            return RecView::idle();
        };
        let info = recorder.recording();
        RecView {
            state: match info.state {
                RecordingState::Idle => RecState::Idle,
                RecordingState::Starting => RecState::Starting,
                RecordingState::Recording => RecState::Recording,
            },
            seconds: info.seconds as f32,
        }
    }

    /// Starts or stops recording the session.
    fn toggle_record(&mut self) {
        if let Some(recorder) = &self.recorder {
            if recorder.recording().state != RecordingState::Idle {
                recorder.stop_recording();
                return;
            }
        }
        if self.record_when_ready {
            self.record_when_ready = false; // a second press cancels the wait
            return;
        }
        // Recording rides on the replay buffer's encoder: switch it on if the
        // user had turned it off, and start once it is running.
        if self.recorder.is_none() {
            self.settings.replay_enabled = true;
            self.sync_replay();
            self.save_settings();
        }
        self.record_when_ready = true;
    }

    /// Opens the file once the recorder is running.
    fn begin_recording(&mut self) {
        let Some(recorder) = &self.recorder else {
            self.record_when_ready = false;
            return;
        };
        match recorder.status() {
            RecorderStatus::Recording => {}
            RecorderStatus::Starting => return, // keep waiting
            RecorderStatus::Unsupported(_) | RecorderStatus::Failed(_) => {
                self.record_when_ready = false;
                self.toast(i18n::Key::ToastRecFailed, ToastKind::Error);
                return;
            }
        }
        self.record_when_ready = false;
        let folder = self.clips_folder();
        if disk_free_bytes(&folder).is_some_and(|free| free < DISK_START_MIN_BYTES) {
            self.toast(i18n::Key::ToastRecNoSpace, ToastKind::Error);
            return;
        }
        let name = clips::recording_file_name(clips::local_time_now());
        let path = clips::unique_path(&folder, &name);
        match recorder.start_recording(path) {
            Ok(()) => self.toast(i18n::Key::ToastRecStarted, ToastKind::Info),
            Err(e) => {
                eprintln!("recording not started: {e}");
                self.toast(i18n::Key::ToastRecFailed, ToastKind::Error);
            }
        }
    }

    fn mic_view(&self) -> MicView {
        let state = match self.mic.as_ref().map(MicCapture::status) {
            None => MicState::Off,
            Some(AudioStatus::Starting) => MicState::Starting,
            Some(AudioStatus::Running) => MicState::Live,
            Some(AudioStatus::Stopped) => MicState::Off,
            Some(AudioStatus::Disconnected(_) | AudioStatus::Failed(_)) => MicState::Unavailable,
        };
        MicView {
            state,
            level: self.mic_level,
        }
    }

    /// Opens or closes the microphone. It needs the replay buffer, which is
    /// what carries its audio.
    fn set_mic(&mut self, on: bool) {
        if !on {
            self.mic = None;
            if let Some(recorder) = &self.recorder {
                recorder.clear_mic();
            }
            self.mic_level = 0.0;
            return;
        }
        let Some(recorder) = &self.recorder else {
            self.toast(i18n::Key::ToastTurnOnFirst, ToastKind::Info);
            return;
        };
        let device = self.settings.last_mic_device_id.clone();
        self.mic = Some(MicCapture::start(device, 1.0, recorder.mic_tap()));
    }

    /// The longest replay this PC's memory allows, in seconds.
    fn replay_limit(&self) -> u32 {
        settings::max_replay_seconds(self.settings.replay_bitrate_mbps, self.total_ram)
    }

    /// Brings a saved length that is too long for this PC down to the limit.
    fn clamp_replay_length(&mut self) {
        self.settings.replay_seconds = self.settings.replay_seconds.min(self.replay_limit());
    }

    /// Starts the buffer over, for a change that needs a new encoder. The
    /// microphone, which feeds the old recorder, is reopened on the new one.
    fn restart_recorder(&mut self) {
        let mic_on = self.mic.is_some();
        self.mic = None;
        self.recorder = None;
        self.sync_replay();
        if mic_on {
            self.set_mic(true);
        }
    }

    fn window_hwnd(&self) -> Option<HWND> {
        match self.window.as_ref()?.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => Some(HWND(h.hwnd.get() as *mut c_void)),
            _ => None,
        }
    }

    /// Lets the user pick the folder for clips and recordings.
    fn choose_folder(&mut self) {
        let start = self.clips_folder();
        let owner = self.window_hwnd();
        let Some(folder) = clips::pick_folder(owner, &start) else {
            return; // cancelled
        };
        if !clips::is_writable_folder(&folder) {
            self.toast(i18n::Key::ToastFolderNotWritable, ToastKind::Error);
            return;
        }
        self.settings.replay_folder = Some(folder.display().to_string());
        self.needs_redraw = true;
    }

    /// Opens Explorer on the newest file saved this session, or on the folder
    /// when there is none (or it was moved).
    fn show_saved(&self) {
        match &self.last_saved {
            Some(file) if file.exists() => clips::reveal_file(file),
            _ => clips::open_folder(&self.clips_folder()),
        }
    }

    /// Folder clips are saved to.
    fn clips_folder(&self) -> std::path::PathBuf {
        self.settings
            .replay_folder
            .as_deref()
            .map_or_else(clips::default_folder, std::path::PathBuf::from)
    }

    fn replay_view(&self) -> ui::controls::ReplayView {
        use ui::controls::{ReplayState, ReplayView};
        let mut view = ReplayView::off(hotkey::LABEL);
        view.keep_seconds = self.settings.replay_seconds;
        view.bitrate_mbps = self.settings.replay_bitrate_mbps;
        view.memory_bytes = settings::replay_memory_bytes(
            self.settings.replay_seconds,
            self.settings.replay_bitrate_mbps,
        );
        view.at_limit = self.settings.replay_seconds >= self.replay_limit();
        view.folder = self.clips_folder().display().to_string();
        view.saving = self.saving.is_some();
        if let Some(recorder) = &self.recorder {
            view.state = match recorder.status() {
                RecorderStatus::Starting => ReplayState::Starting,
                RecorderStatus::Recording => ReplayState::Recording,
                RecorderStatus::Unsupported(_) | RecorderStatus::Failed(_) => {
                    ReplayState::Unavailable
                }
            };
            view.buffered_seconds = recorder.buffered_seconds() as f32;
            view.buffered_mb = recorder.buffered_bytes() as f32 / 1e6;
        }
        view
    }

    fn toast(&mut self, key: i18n::Key, kind: ToastKind) {
        let lang = self.lang_mode().resolve(self.system_lang);
        self.toasts.show(i18n::tr(lang, key), kind);
        self.needs_redraw = true;
    }

    fn save_replay(&mut self) {
        if self.saving.is_some() {
            return;
        }
        let Some(recorder) = &self.recorder else {
            self.toast(i18n::Key::ToastTurnOnFirst, ToastKind::Info);
            return;
        };
        let folder = self.clips_folder();
        if std::fs::create_dir_all(&folder).is_err() {
            self.toast(i18n::Key::ToastClipFailed, ToastKind::Error);
            return;
        }
        let name = clips::clip_file_name(clips::local_time_now());
        let path = clips::unique_path(&folder, &name);
        self.saving = Some(recorder.save(f64::from(self.settings.replay_seconds), path));
        self.toast(i18n::Key::ToastSaving, ToastKind::Info);
    }

    /// Per-tick work: the hotkey and finished saves.
    fn poll_replay(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.mic_level_at).as_secs_f32().min(0.2);
        self.mic_level_at = now;
        // Rise at once, fall smoothly.
        let raw = self.mic.as_ref().map_or(0.0, MicCapture::level);
        self.mic_level = raw.max(self.mic_level - dt * 1.5).clamp(0.0, 1.0);
        if self.hotkey.as_ref().is_some_and(|h| h.take_pressed()) {
            self.save_replay();
        }
        if self
            .hotkey
            .as_ref()
            .is_some_and(|h| h.take_record_pressed())
        {
            self.toggle_record();
        }
        if self.record_when_ready {
            self.begin_recording();
        }
        let mut outcomes = Vec::new();
        if let Some(recorder) = &self.recorder {
            while let Some(outcome) = recorder.take_recording_result() {
                outcomes.push(outcome);
            }
        }
        for outcome in outcomes {
            if let Err(e) = &outcome.result {
                eprintln!("recording ended with an error: {e}");
            }
            if let Ok(clip) = &outcome.result {
                self.last_saved = Some(clip.path.clone());
            }
            if let Some((key, kind)) = outcome_toast(outcome.reason, outcome.result.is_ok()) {
                self.toast(key, kind);
            }
        }
        // Warn once when the disk is running low.
        let info = self.recorder.as_ref().map(|r| r.recording());
        match info {
            Some(info) if info.state == RecordingState::Idle => self.low_disk_warned = false,
            Some(info) if info.low_disk && !self.low_disk_warned => {
                self.low_disk_warned = true;
                self.toast(i18n::Key::ToastLowDisk, ToastKind::Error);
            }
            _ => {}
        }
        if let Some(result) = self.saving.as_ref().and_then(SaveHandle::poll) {
            self.saving = None;
            match result {
                Ok(clip) => {
                    self.last_saved = Some(clip.path);
                    self.toast(i18n::Key::ToastClipSaved, ToastKind::Success);
                }
                Err(e) => {
                    eprintln!("clip not saved: {e}");
                    self.toast(i18n::Key::ToastClipFailed, ToastKind::Error);
                }
            }
        }
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
        if self.cli.test_pattern {
            return; // the synthetic source stays; only the selection changes
        }
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
            CaptureOptions {
                prefer_compressed: self.settings.video_mode == VideoModePref::Compressed,
                ..CaptureOptions::default()
            },
            Arc::clone(&self.slot),
        )
    }

    /// Starts the audio pipeline for the audio device at `index` (or stops).
    fn select_audio(&mut self, index: Option<usize>) {
        self.audio_index = index;
        if self.cli.test_pattern {
            return;
        }
        self.audio = None;
        let Some(device) = index.and_then(|i| self.audio_devices.get(i)) else {
            return;
        };
        self.settings.last_audio_device_id = Some(device.device_id.clone());
        let audio = AudioEngine::start(
            device.device_id.clone(),
            self.settings.volume,
            self.settings.audio_offset_ms,
        );
        if let Some(recorder) = &self.recorder {
            audio.set_tap(Some(recorder.audio_tap()));
        }
        self.audio = Some(audio);
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
        if self.cli.ui_preview {
            return;
        }
        self.settings.was_fullscreen = self.fullscreen;
        settings::save(&self.settings);
    }

    fn theme_mode(&self) -> ThemeMode {
        self.cli.theme.unwrap_or(self.settings.theme)
    }

    fn lang_mode(&self) -> LangMode {
        self.cli.lang.unwrap_or(self.settings.language)
    }

    /// Fake devices so the interface can be explored without hardware.
    fn install_preview_devices(&mut self) {
        let video = |name: &str, link: &str| VideoDevice {
            friendly_name: name.into(),
            symbolic_link: link.into(),
        };
        let audio = |name: &str, id: &str| AudioDevice {
            friendly_name: name.into(),
            device_id: id.into(),
        };
        self.video_devices = vec![
            video("UGREEN 25173", "preview:video:0"),
            video("Integrated Camera", "preview:video:1"),
        ];
        self.audio_devices = vec![
            audio("HDMI (UGREEN 25173)", "preview:audio:0"),
            audio("Microphone (Integrated Camera)", "preview:audio:1"),
            audio("Desktop Microphone (RØDE NT-USB Mini)", "preview:audio:2"),
        ];
        self.video_index = Some(0);
        self.audio_index = Some(0);
    }

    /// Snapshot of everything the controls display.
    fn model(&self) -> Model {
        let status = match &self.source {
            Source::Test(_) => StatusKind::Live,
            Source::None => StatusKind::Idle,
            Source::Capture { capture, .. } => match capture.status() {
                CaptureStatus::Running { .. } | CaptureStatus::Stopped => StatusKind::Live,
                CaptureStatus::Starting => StatusKind::Connecting,
                CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_) => {
                    StatusKind::Problem
                }
            },
        };
        let resolution = if self.video_size.0 > 0 {
            // The rate is unknown for the first second; show just the height.
            if self.fps >= 1.0 {
                format!("{}p · {:.0} fps", self.video_size.1, self.fps)
            } else {
                format!("{}p", self.video_size.1)
            }
        } else {
            String::new()
        };
        let appearance = self.theme_mode().resolve(self.system_appearance);
        let names = |devices: &[String]| devices.to_vec();
        Model {
            theme: Theme::for_appearance(appearance),
            lang: self.lang_mode().resolve(self.system_lang),
            status,
            resolution,
            volume: self.settings.volume,
            sync_ms: self.settings.audio_offset_ms,
            fullscreen: self.fullscreen,
            video_devices: names(
                &self
                    .video_devices
                    .iter()
                    .map(|d| d.friendly_name.clone())
                    .collect::<Vec<_>>(),
            ),
            video_selected: self.video_index,
            audio_devices: names(
                &self
                    .audio_devices
                    .iter()
                    .map(|d| d.friendly_name.clone())
                    .collect::<Vec<_>>(),
            ),
            audio_selected: self.audio_index,
            preset: self.settings.render_preset,
            theme_mode: self.theme_mode(),
            lang_mode: self.lang_mode(),
            video_mode: self.settings.video_mode,
            stats_open: self.stats_open,
            replay: self.replay_view(),
            mic_devices: self.mic_device_names(self.lang_mode().resolve(self.system_lang)),
            mic_selected: Some(self.mic_selected()),
            mic: self.mic_view(),
            rec: self.rec_view(),
        }
    }

    /// Applies a change requested through the controls.
    fn apply(&mut self, action: Action) {
        match action {
            Action::SetVolume(volume) => {
                self.settings.volume = volume;
                if let Some(audio) = &self.audio {
                    audio.set_volume(volume);
                }
            }
            Action::SetSync(ms) => {
                self.settings.audio_offset_ms = ms;
                if let Some(audio) = &self.audio {
                    audio.set_offset_ms(ms);
                }
            }
            Action::ToggleFullscreen => {
                self.set_fullscreen(!self.fullscreen);
                return;
            }
            Action::SelectVideo(index) => {
                self.select_video(Some(index));
                // Follow the new video device with its matching audio.
                if let Some(audio) = self.match_audio(Some(index)) {
                    self.select_audio(Some(audio));
                }
            }
            Action::SelectAudio(index) => self.select_audio(Some(index)),
            Action::ToggleReplay
                if self
                    .recorder
                    .as_ref()
                    .is_some_and(|r| r.recording().state != RecordingState::Idle) =>
            {
                // Turning the buffer off would cut the recording short.
                self.toast(i18n::Key::ToastStopRecordingFirst, ToastKind::Info);
                return;
            }
            Action::ToggleReplay => {
                self.settings.replay_enabled = !self.settings.replay_enabled;
                self.sync_replay();
                let key = if self.settings.replay_enabled {
                    i18n::Key::ToastReplayOn
                } else {
                    i18n::Key::ToastReplayOff
                };
                self.toast(key, ToastKind::Info);
            }
            Action::ToggleRecord => {
                self.toggle_record();
                return;
            }
            Action::ToggleMic => {
                let on = self.mic.is_none();
                self.set_mic(on);
                if self.mic.is_some() == on {
                    let key = if on {
                        i18n::Key::ToastMicOn
                    } else {
                        i18n::Key::ToastMicOff
                    };
                    self.toast(key, ToastKind::Info);
                }
                return; // the microphone always starts off; nothing to save
            }
            Action::SelectMic(index) => {
                self.settings.last_mic_device_id = index
                    .checked_sub(1)
                    .and_then(|i| self.audio_devices.get(i))
                    .map(|d| d.device_id.clone());
                if self.mic.is_some() {
                    // Reopen on the new device.
                    self.set_mic(false);
                    self.set_mic(true);
                }
            }
            Action::ReplayLength(delta) => {
                self.settings.replay_seconds =
                    settings::step_replay_seconds(self.settings.replay_seconds, delta);
                self.clamp_replay_length();
                if let Some(recorder) = &self.recorder {
                    recorder.set_keep_seconds(f64::from(
                        self.settings.replay_seconds + settings::REPLAY_MARGIN_SECONDS,
                    ));
                }
            }
            Action::SaveReplay => {
                self.save_replay();
                return;
            }
            Action::OpenReplayFolder => {
                self.show_saved();
                return;
            }
            Action::ChooseFolder => self.choose_folder(),
            Action::SetQuality(quality) => {
                if self
                    .recorder
                    .as_ref()
                    .is_some_and(|r| r.recording().state != RecordingState::Idle)
                {
                    // A new encoder would cut the recording short.
                    self.toast(i18n::Key::ToastStopRecordingFirst, ToastKind::Info);
                    return;
                }
                if self.settings.replay_bitrate_mbps != quality.mbps() {
                    self.settings.replay_bitrate_mbps = quality.mbps();
                    self.clamp_replay_length();
                    if self.recorder.is_some() {
                        self.restart_recorder();
                        self.toast(i18n::Key::ToastQualityChanged, ToastKind::Info);
                    }
                }
            }
            Action::ToggleStats => {
                self.stats_open = !self.stats_open;
                return;
            }
            Action::SetVideoMode(mode) => {
                self.settings.video_mode = mode;
                // A deliberate choice ends any automatic switch and allows a
                // new automatic attempt later (for example after changing port).
                self.auto_switched = false;
                self.fallback_tried = false;
                self.switch_check = None;
                self.fallback.reset();
                self.select_video(self.video_index);
            }
            Action::SetPreset(preset) => {
                self.settings.render_preset = preset;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_filter(filter_for(preset));
                }
            }
            Action::SetTheme(mode) => {
                self.settings.theme = mode;
                self.cli.theme = None; // an explicit choice wins over --theme
            }
            Action::SetLang(mode) => {
                self.settings.language = mode;
                self.cli.lang = None;
            }
        }
        self.save_settings();
    }

    /// Refreshes the measurements once a second.
    fn update_stats(&mut self) {
        let elapsed = self.last_stats.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }
        let secs = elapsed.as_secs_f32();
        let published = self.slot.published();
        let dropped = self.slot.dropped();
        let received_fps = (published - self.last_published) as f32 / secs;
        let dropped_per_sec = (dropped - self.last_dropped) as f32 / secs;
        let shown_fps = (self.rendered - self.last_rendered) as f32 / secs;
        self.last_published = published;
        self.last_dropped = dropped;
        self.last_rendered = self.rendered;
        self.last_stats = Instant::now();
        self.stats_updates += 1;
        self.fps = shown_fps;

        let usage = self.sampler.sample();
        let latency_ms = (self.latency_count > 0)
            .then(|| (self.latency_sum_ms / f64::from(self.latency_count)) as f32)
            .or(self.stats.latency_ms);
        self.latency_sum_ms = 0.0;
        self.latency_count = 0;

        let native = self.native_mode();
        let video = match &self.source {
            Source::Test(_) => VideoState::Running,
            Source::None => VideoState::NoDevice,
            Source::Capture { capture, .. } => match capture.status() {
                CaptureStatus::Running { .. } | CaptureStatus::Stopped => VideoState::Running,
                CaptureStatus::Starting => VideoState::Connecting,
                CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_) => VideoState::Lost,
            },
        };
        let monitor_hz = self
            .window
            .as_ref()
            .and_then(|w| w.current_monitor())
            .and_then(|m| m.refresh_rate_millihertz())
            .map(|mhz| mhz as f32 / 1000.0);
        let gpu_ms = self.renderer.as_ref().and_then(Renderer::gpu_ms);
        let audio_ok = self.audio.as_ref().map(|a| {
            !matches!(
                a.status(),
                AudioStatus::Disconnected(_) | AudioStatus::Failed(_)
            )
        });

        let findings = diagnose(&Snapshot {
            video,
            mode_fps: native.as_ref().map(|n| n.fps),
            mode_raw_mbps: native.as_ref().and_then(StreamInfo::raw_mbps),
            received_fps,
            dropped_per_sec,
            cpu_core_percent: usage.cpu_core_percent,
            gpu_ms,
            frame_budget_ms: monitor_hz.map(|hz| 1000.0 / hz),
            audio_ok,
        });

        let has_compressed = self.capture_has_compressed();
        let eligible = self.settings.video_mode == VideoModePref::Auto
            && video == VideoState::Running
            && has_compressed
            && !self.fallback_tried
            && !self.cli.test_pattern;
        let usb_slow = findings.contains(&Finding::NeedsUsb3);
        if self.fallback.observe(usb_slow, eligible) {
            self.switch_to_compressed(received_fps);
        }
        self.judge_switch(received_fps);
        let mut findings = findings;
        if self.auto_switched {
            findings.insert(0, Finding::SwitchedToCompressed);
        }

        let findings = if self.cli.ui_preview && self.cli.demo_findings {
            use dockview_core::diagnostics::Finding;
            vec![
                Finding::NeedsUsb3,
                Finding::DroppingFrames,
                Finding::HighCpu,
            ]
        } else {
            findings
        };

        let size = self
            .window
            .as_ref()
            .map_or((0, 0), |w| (w.inner_size().width, w.inner_size().height));
        self.stats = Stats {
            device_mode: native.as_ref().map(ToString::to_string),
            mode_mbps: native.as_ref().and_then(StreamInfo::raw_mbps),
            received_fps,
            shown_fps,
            dropped_per_sec,
            latency_ms,
            cpu_core_percent: usage.cpu_core_percent,
            cpu_total_percent: usage.cpu_total_percent,
            gpu_ms,
            memory_mb: usage.memory_mb,
            audio_buffer_ms: self.audio.as_ref().map(AudioEngine::buffered_ms),
            monitor_hz,
            window: size,
            findings,
            measured: true,
        };
        if let Some(log) = self.stats_log.as_mut() {
            log.record(&self.stats);
        }
    }

    /// Whether the open device also offers a compressed mode.
    fn capture_has_compressed(&self) -> bool {
        match &self.source {
            Source::Capture { capture, .. } => matches!(
                capture.status(),
                CaptureStatus::Running {
                    has_compressed: true,
                    ..
                }
            ),
            _ => false,
        }
    }

    /// Moves to a compressed mode because the USB port cannot carry the
    /// uncompressed one, and schedules a check that it actually helped.
    fn switch_to_compressed(&mut self, fps_before: f32) {
        eprintln!("[auto] USB too slow for the uncompressed mode ({fps_before:.0} fps): switching to a compressed mode");
        self.settings.video_mode = VideoModePref::Compressed;
        self.auto_switched = true;
        self.fallback_tried = true;
        self.switch_check = Some((SWITCH_CHECK_SECS, fps_before));
        self.save_settings();
        self.select_video(self.video_index);
    }

    /// A few seconds after an automatic switch, keeps it only if the rate
    /// clearly improved; otherwise the slowness had another cause, so the
    /// better-quality mode comes back.
    fn judge_switch(&mut self, fps_now: f32) {
        let Some((remaining, before)) = self.switch_check else {
            return;
        };
        if remaining > 1 {
            self.switch_check = Some((remaining - 1, before));
            return;
        }
        self.switch_check = None;
        eprintln!("[auto] compressed mode: {fps_now:.0} fps (was {before:.0} fps)");
        if !switch_helped(before, fps_now) {
            eprintln!("[auto] no clear improvement: going back to automatic mode");
            self.settings.video_mode = VideoModePref::Auto;
            self.auto_switched = false;
            self.save_settings();
            self.select_video(self.video_index);
        }
    }

    /// The device mode in use (a stand-in for the synthetic source).
    fn native_mode(&self) -> Option<StreamInfo> {
        match &self.source {
            Source::Test(_) => Some(StreamInfo {
                type_index: 0,
                format_name: "NV12".into(),
                width: 1920,
                height: 1080,
                fps: 30,
                is_compressed: false,
            }),
            Source::Capture { capture, .. } => match capture.status() {
                CaptureStatus::Running { native, .. } => Some(native),
                _ => None,
            },
            Source::None => None,
        }
    }

    /// Pointer shape and visibility follow what the controls are doing.
    fn update_cursor(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let pointer = self.controls.wants_pointer();
        if pointer != self.cursor_pointer {
            self.cursor_pointer = pointer;
            window.set_cursor(if pointer {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            });
        }
        // While watching full screen, an idle cursor gets out of the way.
        let hidden = self.fullscreen && !self.controls.is_visible();
        if hidden != self.cursor_hidden {
            self.cursor_hidden = hidden;
            window.set_cursor_visible(!hidden);
        }
    }

    fn render(&mut self) {
        let model = self.model();
        let rec = self.rec_view();
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let pointer = self.pointer;
        let scale = self.ui_scale;
        // The first seconds include start-up work; wait for steady numbers.
        let settled = !self.cli.show_stats || self.stats_updates >= 3;
        let capture =
            self.cli.screenshot.is_some() && self.rendered >= SCREENSHOT_AFTER_FRAMES && settled;

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        // Wait for a free present slot *before* picking the frame so we always
        // show the freshest one.
        renderer.wait_for_present_slot(100);
        let mut arrived = None;
        if let Some(frame) = self.slot.take_latest() {
            arrived = Some(frame.arrived);
            self.video_size = (frame.width, frame.height);
            if let Err(e) = renderer.upload(&frame) {
                eprintln!("upload failed: {e}");
            }
            self.slot.recycle(frame);
            self.rendered += 1;
        }

        let (ui, ui_gpu, controls) = (&mut self.ui, &mut self.ui_gpu, &mut self.controls);
        let (stats_panel, stats) = (&mut self.stats_panel, &self.stats);
        let (stats_open, animations) = (self.stats_open, self.animations);
        let toasts = &mut self.toasts;
        let result = renderer.draw(
            &mut |_device, context, size| {
                let Some(gpu) = ui_gpu.as_mut() else {
                    return;
                };
                ui.begin(size, scale);
                controls.frame(ui, &model, &pointer, dt);
                stats_panel.frame(
                    ui,
                    &model.theme,
                    model.lang,
                    stats,
                    stats_open,
                    dt,
                    animations,
                );
                if rec.is_on() {
                    ui::rec::draw(
                        ui,
                        &model.theme,
                        rec.seconds,
                        rec.state == RecState::Starting,
                        animations,
                    );
                }
                toasts.frame(ui, &model.theme, dt, animations);
                if let Err(e) = gpu.render(context, ui, size) {
                    eprintln!("ui draw failed: {e}");
                }
            },
            capture,
        );
        match result {
            Ok(Some(shot)) => self.save_screenshot(&shot),
            Ok(None) => {}
            Err(e) => eprintln!("present failed: {e}"),
        }
        if let Some(arrived) = arrived {
            self.latency_sum_ms += arrived.elapsed().as_secs_f64() * 1000.0;
            self.latency_count += 1;
        }

        self.pointer.end_frame();
        for action in self.controls.take_actions() {
            self.apply(action);
        }
        self.update_cursor();
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

    fn on_key(&mut self, key: &Key) {
        match key {
            Key::Named(NamedKey::F11) => self.set_fullscreen(!self.fullscreen),
            Key::Named(NamedKey::Escape) => {
                if !self.controls.close_settings() && self.fullscreen {
                    self.set_fullscreen(false);
                }
            }
            Key::Named(NamedKey::ArrowUp) => self.change_volume(VOLUME_STEP),
            Key::Named(NamedKey::ArrowDown) => self.change_volume(-VOLUME_STEP),
            Key::Named(NamedKey::ArrowLeft) => self.change_offset(-OFFSET_STEP_MS),
            Key::Named(NamedKey::ArrowRight) => self.change_offset(OFFSET_STEP_MS),
            Key::Character(c) => match c.to_ascii_lowercase().as_str() {
                "f" => self.set_fullscreen(!self.fullscreen),
                "p" => self.cycle_preset(),
                "i" => self.stats_open = !self.stats_open,
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
                self.needs_redraw = true;
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(e) = renderer.resize(size.width, size.height) {
                        eprintln!("resize failed: {e}");
                    }
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::CursorMoved { position, .. } => {
                let scale = f64::from(self.ui_scale);
                self.pointer.pos = Some(((position.x / scale) as f32, (position.y / scale) as f32));
                self.needs_redraw = true;
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer.pos = None;
                self.needs_redraw = true;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                match state {
                    ElementState::Pressed => self.pointer.press(),
                    ElementState::Released => self.pointer.release(),
                }
                self.needs_redraw = true;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.pointer.wheel += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 40.0) as f32,
                };
                self.needs_redraw = true;
            }
            WindowEvent::ThemeChanged(theme) => self.system_appearance = appearance_of(Some(theme)),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ui_scale = self.cli.scale.unwrap_or(scale_factor as f32);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.on_key(&event.logical_key);
                self.needs_redraw = true;
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
        self.update_stats();
        self.poll_replay();
        // Redraw for new video, for input, and while the controls are on
        // screen (hover, drag and fade animations need steady frames).
        let new_frame = self.slot.wait_for_frame(FRAME_WAIT);
        let animating =
            self.controls.is_visible() || self.toasts.is_visible() || self.rec_view().is_on();
        if new_frame || animating || std::mem::take(&mut self.needs_redraw) {
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

    // The preview must look the same on every machine: ignore saved settings.
    let saved = if cli.ui_preview {
        AppSettings::default()
    } else {
        settings::load()
    };
    let mut app = App::new(slot, saved, video_devices, audio_devices, cli);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("event loop error: {e}");
    }
}

/// The message to show when a recording (or one of its files) ends, if any.
fn outcome_toast(reason: EndReason, saved: bool) -> Option<(i18n::Key, ToastKind)> {
    use i18n::Key;
    match (reason, saved) {
        // The user cancelled before anything was recorded: nothing to say.
        (EndReason::Cancelled, _) => None,
        (EndReason::ResolutionChanged, true) => Some((Key::ToastRecNewPart, ToastKind::Info)),
        (EndReason::DiskFull, true) => Some((Key::ToastRecDiskFull, ToastKind::Error)),
        (EndReason::NoSignal, _) => Some((Key::ToastRecNoSignal, ToastKind::Error)),
        (_, true) => Some((Key::ToastRecSaved, ToastKind::Success)),
        (_, false) => Some((Key::ToastRecFailed, ToastKind::Error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_way_a_recording_can_end_says_the_right_thing() {
        use i18n::Key;
        assert_eq!(outcome_toast(EndReason::Cancelled, false), None);
        assert_eq!(
            outcome_toast(EndReason::Stopped, true),
            Some((Key::ToastRecSaved, ToastKind::Success))
        );
        assert_eq!(
            outcome_toast(EndReason::ResolutionChanged, true),
            Some((Key::ToastRecNewPart, ToastKind::Info))
        );
        assert_eq!(
            outcome_toast(EndReason::DiskFull, true),
            Some((Key::ToastRecDiskFull, ToastKind::Error))
        );
        assert_eq!(
            outcome_toast(EndReason::NoSignal, false),
            Some((Key::ToastRecNoSignal, ToastKind::Error))
        );
        assert_eq!(
            outcome_toast(EndReason::Failed, false),
            Some((Key::ToastRecFailed, ToastKind::Error))
        );
    }

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
