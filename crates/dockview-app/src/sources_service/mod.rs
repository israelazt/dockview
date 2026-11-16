//! The sources: what can be captured and heard (the capture cards, the audio
//! inputs, the monitors and the windows) and what is being captured: the
//! capture card, a screen, a window or the test pattern, kept alive and opened
//! again when it drops out.
//!
//! The service owns that state and the machine the sources are started on.
//! `Core` asks it things and tells it what to do; what also touches the
//! settings, the window or the events (choosing a card and remembering it, say)
//! stays in `Core`, which coordinates.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dockview_domain::device::{match_audio_to_video, AudioDevice, VideoDevice};
use dockview_domain::diagnostics::VideoState;
use dockview_domain::events::AppEvent;
use dockview_domain::frame::FrameSlot;
use dockview_domain::i18n::Lang;
use dockview_domain::ports::{AudioPlayback, CaptureHandle, MicInput, ScreenHandle, SourceMachine};
use dockview_domain::scene::SourceRef;
use dockview_domain::sources::{
    CaptureOptions, CaptureStatus, CaptureTarget, MonitorInfo, ScreenOptions, ScreenStatus,
    WindowInfo,
};
use dockview_domain::stream::StreamInfo;
use dockview_domain::view::SourcesView;

use crate::sources::{
    find_window, monitor_label, video_state_of_capture, video_state_of_screen, Reconnect,
    SourceKind,
};

mod audio;
#[cfg(test)]
mod tests;

/// What is producing the main picture.
pub enum Source {
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

impl Source {
    /// What the diagnosis and the status light should know about this source.
    pub fn video_state(&self) -> VideoState {
        match self {
            Self::Test(_) => VideoState::Running,
            Self::None => VideoState::NoDevice,
            Self::Capture { capture, .. } => video_state_of_capture(&capture.status()),
            Self::Screen(capture) => video_state_of_screen(&capture.status()),
        }
    }
}

/// What [`SourcesService::maintain`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Maintained {
    /// Nothing to do (a lost card was reopened by the service itself).
    Fine,
    /// The screen or window being captured is gone; the caller goes back to
    /// the card and tells the user with this event.
    ScreenLost(AppEvent),
}

pub struct SourcesService {
    /// What the sources are started on (the real machine, or a fake in tests).
    pub(crate) machine: Box<dyn SourceMachine>,
    pub(crate) video_devices: Vec<VideoDevice>,
    pub(crate) audio_devices: Vec<AudioDevice>,
    /// The capture card in use, as a position in `video_devices`.
    pub(crate) video_index: Option<usize>,
    /// The audio input in use, as a position in `audio_devices`.
    pub(crate) audio_index: Option<usize>,
    /// What can be captured, refreshed while the source popover is open.
    pub(crate) monitors: Vec<MonitorInfo>,
    pub(crate) windows: Vec<WindowInfo>,
    refreshed: Instant,
    /// What is producing the main picture.
    pub(crate) source: Source,
    /// Whether the card, a monitor or a window is being captured.
    pub(crate) source_kind: SourceKind,
    /// Where the main source publishes. It forwards to the output slot unless
    /// a scene is running, when it keeps the frames for the compositor.
    pub(crate) input_slot: Arc<FrameSlot>,
    /// The card's sound, played to the speakers (see `audio`).
    audio: Option<Box<dyn AudioPlayback>>,
    /// The sound of the PC or of one program, while a screen is captured.
    loopback: Option<Box<dyn MicInput>>,
    /// When to start the card's sound again after it was lost.
    audio_reconnect: Reconnect,
    /// Whether the user was told the sound was lost (so the return is told too).
    audio_lost: bool,
}

impl SourcesService {
    pub fn new(
        machine: Box<dyn SourceMachine>,
        video_devices: Vec<VideoDevice>,
        audio_devices: Vec<AudioDevice>,
        input_slot: Arc<FrameSlot>,
    ) -> Self {
        Self {
            machine,
            video_devices,
            audio_devices,
            video_index: None,
            audio_index: None,
            monitors: Vec::new(),
            windows: Vec::new(),
            refreshed: Instant::now(),
            source: Source::None,
            source_kind: SourceKind::Capture,
            input_slot,
            audio: None,
            loopback: None,
            audio_reconnect: Reconnect::new(),
            audio_lost: false,
        }
    }

    // -- What can be captured ---------------------------------------------

    /// Lists the monitors and the windows again. The program's own windows are
    /// left out: capturing the viewer inside itself is no use.
    pub fn refresh(&mut self) {
        self.monitors = self.machine.list_monitors();
        let own = std::process::id();
        self.windows = self
            .machine
            .list_windows()
            .into_iter()
            .filter(|w| w.process_id != own)
            .collect();
        self.refreshed = Instant::now();
    }

    /// Whether the monitors and windows were last listed more than `interval`
    /// before `now`.
    pub fn lists_are_stale(&self, now: Instant, interval: Duration) -> bool {
        now.saturating_duration_since(self.refreshed) > interval
    }

    /// The audio input that belongs to the capture card at `video` (they share
    /// a name), as a position in `audio_devices`.
    pub fn match_audio(&self, video: Option<usize>) -> Option<usize> {
        let matched = match_audio_to_video(
            video.and_then(|i| self.video_devices.get(i)),
            &self.audio_devices,
        )?;
        self.audio_devices.iter().position(|a| a == matched)
    }

    /// The monitors and windows for the source popover.
    pub fn view(&self, lang: Lang) -> SourcesView {
        let monitors = self
            .monitors
            .iter()
            .enumerate()
            .map(|(i, m)| monitor_label(lang, i, m))
            .collect();
        let windows = self.windows.iter().map(|w| w.title.clone()).collect();
        let current = self.source_kind.current(&self.monitors, &self.windows);
        SourcesView {
            monitors,
            windows,
            current,
        }
    }

    /// A window (its handle and process) by title: an exact match first,
    /// then one that contains the text. Never one of our own.
    pub fn find_window(&self, title: &str) -> Option<(isize, u32)> {
        find_window(&self.windows, title, std::process::id()).map(|w| (w.handle, w.process_id))
    }

    /// The main source as something that can be saved in a scene.
    pub fn main_source_ref(&self) -> Option<SourceRef> {
        match self.source_kind {
            SourceKind::Capture => self
                .video_index
                .and_then(|i| self.video_devices.get(i))
                .map(|d| SourceRef::Capture {
                    symbolic_link: d.symbolic_link.clone(),
                }),
            SourceKind::Monitor(handle) => {
                self.monitors
                    .iter()
                    .find(|m| m.handle == handle)
                    .map(|m| SourceRef::Monitor {
                        device_name: m.device_name.clone(),
                    })
            }
            SourceKind::Window { handle, .. } => self
                .windows
                .iter()
                .find(|w| w.handle == handle)
                .map(|w| SourceRef::Window {
                    title: w.title.clone(),
                }),
        }
    }

    // -- What is being captured -------------------------------------------

    /// What the diagnosis and the status light should know about the source.
    pub fn video_state(&self) -> VideoState {
        self.source.video_state()
    }

    /// Stops the main source (a capture card is released).
    pub fn release(&mut self) {
        self.source = Source::None;
    }

    /// Starts the colour bars as the main source.
    pub fn open_test_pattern(&mut self, fps: u32) {
        self.source = Source::Test(
            self.machine
                .start_test_pattern(Arc::clone(&self.input_slot), fps),
        );
    }

    /// Opens the capture card `symbolic_link` as the main source.
    pub fn open_card(&mut self, symbolic_link: &str, options: CaptureOptions) {
        self.source = Source::Capture {
            capture: self.start_capture(symbolic_link, options),
            reconnect: Reconnect::new(),
        };
    }

    /// Captures a monitor or a window as the main source.
    pub fn open_screen(&mut self, target: CaptureTarget, kind: SourceKind) {
        self.source = Source::None; // releases the card
        self.source_kind = kind;
        self.source = Source::Screen(self.machine.start_screen(
            target,
            ScreenOptions::default(),
            Arc::clone(&self.input_slot),
        ));
    }

    fn start_capture(
        &self,
        symbolic_link: &str,
        options: CaptureOptions,
    ) -> Box<dyn CaptureHandle> {
        self.machine.start_capture(
            symbolic_link.to_owned(),
            options,
            Arc::clone(&self.input_slot),
        )
    }

    /// Whether the open device also offers a compressed mode.
    pub fn capture_has_compressed(&self) -> bool {
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

    /// The device mode in use (a stand-in for the synthetic source).
    pub fn native_mode(&self) -> Option<StreamInfo> {
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
            Source::Screen(_) | Source::None => None,
        }
    }

    /// Keeps the main source alive: a capture card that dropped out is opened
    /// again after a pause (with `options`); a screen or window that ended is
    /// reported, so the caller can go back to the card.
    pub fn maintain(&mut self, now: Instant, options: CaptureOptions) -> Maintained {
        if let Source::Screen(capture) = &self.source {
            return match capture.status() {
                ScreenStatus::Ended(_) => Maintained::ScreenLost(AppEvent::SourceLost),
                ScreenStatus::Failed(e) => {
                    dockview_domain::log_warn!("screen capture failed: {e}");
                    Maintained::ScreenLost(AppEvent::SourceFailed)
                }
                _ => Maintained::Fine,
            };
        }
        let Some(device) = self.video_index.and_then(|i| self.video_devices.get(i)) else {
            return Maintained::Fine;
        };
        let link = device.symbolic_link.clone();
        let Source::Capture { capture, reconnect } = &mut self.source else {
            return Maintained::Fine;
        };
        let lost = matches!(
            capture.status(),
            CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_)
        );
        if reconnect.tick(lost, now) {
            self.open_card(&link, options);
        }
        Maintained::Fine
    }
}
