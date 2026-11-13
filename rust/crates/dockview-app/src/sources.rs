//! Rules about sources, without threads or windows: what kind of source is
//! being shown, where its sound comes from, when to try a lost device again,
//! how a window is found by its title and how a monitor is named.
//!
//! The app starts and stops the real captures; this decides, so it can be
//! tested.

use std::time::{Duration, Instant};

use dockview_core::capture::CaptureStatus;
use dockview_core::device::{AudioDevice, VideoDevice};
use dockview_core::diagnostics::VideoState;
use dockview_core::screen::{MonitorInfo, ScreenStatus, WindowInfo};

use crate::i18n::{self, Lang};
use crate::ui::controls::{CurrentSource, StatusKind};

/// Delay before trying to reopen a capture device that went away.
pub const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// What is being shown and recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// The selected capture card.
    Capture,
    /// A monitor (its handle).
    Monitor(isize),
    /// A window (its handle) and the program that owns it.
    Window { handle: isize, pid: u32 },
}

/// Where the game's sound comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioRoute {
    /// The capture card's own audio device.
    Card,
    /// Everything the PC plays (a monitor is being captured).
    System,
    /// What one program plays (a window is being captured).
    Process(u32),
}

impl SourceKind {
    pub fn audio_route(self) -> AudioRoute {
        match self {
            Self::Capture => AudioRoute::Card,
            Self::Monitor(_) => AudioRoute::System,
            Self::Window { pid, .. } => AudioRoute::Process(pid),
        }
    }

    /// Which entry of the source popover is the current one.
    pub fn current(self, monitors: &[MonitorInfo], windows: &[WindowInfo]) -> CurrentSource {
        match self {
            Self::Capture => CurrentSource::Capture,
            Self::Monitor(handle) => CurrentSource::Monitor(
                monitors
                    .iter()
                    .position(|m| m.handle == handle)
                    .unwrap_or(0),
            ),
            Self::Window { handle, .. } => {
                CurrentSource::Window(windows.iter().position(|w| w.handle == handle))
            }
        }
    }
}

/// When to try a lost device again: a pause after it goes away, then one try,
/// and the pause starts over if it is still gone.
#[derive(Debug, Default)]
pub struct Reconnect {
    retry_at: Option<Instant>,
}

impl Reconnect {
    pub fn new() -> Self {
        Self::default()
    }

    /// Called often. `lost` says whether the device is gone right now; the
    /// answer says whether to try to open it again at `now`.
    pub fn tick(&mut self, lost: bool, now: Instant) -> bool {
        if !lost {
            self.retry_at = None;
            return false;
        }
        let due = *self.retry_at.get_or_insert(now + RECONNECT_DELAY);
        if now >= due {
            self.retry_at = None;
            true
        } else {
            false
        }
    }
}

/// The window whose title is `title`: an exact match (ignoring case) first,
/// then one that contains the text. Never one of our own (`own_pid`).
pub fn find_window<'a>(
    windows: &'a [WindowInfo],
    title: &str,
    own_pid: u32,
) -> Option<&'a WindowInfo> {
    let wanted = title.to_lowercase();
    let others = || windows.iter().filter(|w| w.process_id != own_pid);
    others()
        .find(|w| w.title.to_lowercase() == wanted)
        .or_else(|| others().find(|w| w.title.to_lowercase().contains(&wanted)))
}

/// How a monitor appears in the list: `Screen 1 · 1920×1080 (primary)`.
pub fn monitor_label(lang: Lang, index: usize, monitor: &MonitorInfo) -> String {
    let primary = if monitor.primary {
        format!(" ({})", i18n::tr(lang, i18n::Key::SourcePrimary))
    } else {
        String::new()
    };
    format!(
        "{} {} · {}×{}{}",
        i18n::tr(lang, i18n::Key::SourceScreenWord),
        index + 1,
        monitor.width,
        monitor.height,
        primary
    )
}

/// What the diagnosis should know about a capture card.
pub fn video_state_of_capture(status: &CaptureStatus) -> VideoState {
    match status {
        CaptureStatus::Running { .. } | CaptureStatus::Stopped => VideoState::Running,
        CaptureStatus::Starting => VideoState::Connecting,
        CaptureStatus::Disconnected { .. } | CaptureStatus::Failed(_) => VideoState::Lost,
    }
}

/// What the diagnosis should know about a screen or window capture.
pub fn video_state_of_screen(status: &ScreenStatus) -> VideoState {
    match status {
        ScreenStatus::Running { .. } => VideoState::Running,
        ScreenStatus::Starting => VideoState::Connecting,
        ScreenStatus::Ended(_) | ScreenStatus::Failed(_) => VideoState::Lost,
    }
}

/// Which video device to start with: the one the command line asked for by
/// name (any part of it, ignoring case), else the one used last time, else the
/// first.
pub fn pick_video(
    devices: &[VideoDevice],
    wanted_name: Option<&str>,
    last_link: Option<&str>,
) -> Option<usize> {
    let by_name = wanted_name.map(str::to_lowercase).and_then(|wanted| {
        devices
            .iter()
            .position(|d| d.friendly_name.to_lowercase().contains(&wanted))
    });
    by_name
        .or_else(|| {
            devices
                .iter()
                .position(|d| last_link == Some(d.symbolic_link.as_str()))
        })
        .or_else(|| (!devices.is_empty()).then_some(0))
}

/// Which audio device to start with: the one used last time, else the one that
/// belongs to the video device (`matched`), else the first.
pub fn pick_audio(
    devices: &[AudioDevice],
    last_id: Option<&str>,
    matched: Option<usize>,
) -> Option<usize> {
    devices
        .iter()
        .position(|d| last_id == Some(d.device_id.as_str()))
        .or(matched.filter(|&i| i < devices.len()))
        .or_else(|| (!devices.is_empty()).then_some(0))
}

/// Which scene the app starts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupScene {
    /// The corner picture asked for on the command line (`--pip`).
    FromCommandLine,
    /// The scene used last time (its position in the saved list).
    Saved(usize),
    /// No scene: the plain main source.
    None,
}

/// The command line wins; otherwise the saved scene comes back, unless the
/// command line chose a different main source or this is the UI preview.
pub fn startup_scene(
    pip_asked: bool,
    chose_main_source: bool,
    ui_preview: bool,
    saved: Option<usize>,
) -> StartupScene {
    if pip_asked {
        StartupScene::FromCommandLine
    } else if chose_main_source || ui_preview {
        StartupScene::None
    } else {
        saved.map_or(StartupScene::None, StartupScene::Saved)
    }
}

/// The status light of the controls for a diagnosis state.
pub fn status_kind(state: VideoState) -> StatusKind {
    match state {
        VideoState::Running => StatusKind::Live,
        VideoState::Connecting => StatusKind::Connecting,
        VideoState::Lost => StatusKind::Problem,
        VideoState::NoDevice => StatusKind::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(handle: isize, title: &str, pid: u32) -> WindowInfo {
        WindowInfo {
            handle,
            title: title.into(),
            process_id: pid,
        }
    }

    fn monitor(handle: isize, primary: bool) -> MonitorInfo {
        MonitorInfo {
            handle,
            device_name: format!(r"\\.\DISPLAY{handle}"),
            width: 1920,
            height: 1080,
            primary,
        }
    }

    #[test]
    fn the_sound_follows_the_kind_of_source() {
        assert_eq!(SourceKind::Capture.audio_route(), AudioRoute::Card);
        assert_eq!(SourceKind::Monitor(3).audio_route(), AudioRoute::System);
        assert_eq!(
            SourceKind::Window { handle: 1, pid: 77 }.audio_route(),
            AudioRoute::Process(77)
        );
    }

    #[test]
    fn the_current_entry_of_the_list_is_found_by_handle() {
        let monitors = [monitor(10, true), monitor(20, false)];
        let windows = [window(5, "A", 1), window(6, "B", 2)];
        assert_eq!(
            SourceKind::Capture.current(&monitors, &windows),
            CurrentSource::Capture
        );
        assert_eq!(
            SourceKind::Monitor(20).current(&monitors, &windows),
            CurrentSource::Monitor(1)
        );
        // A monitor that disappeared falls back to the first one.
        assert_eq!(
            SourceKind::Monitor(99).current(&monitors, &windows),
            CurrentSource::Monitor(0)
        );
        assert_eq!(
            SourceKind::Window { handle: 6, pid: 2 }.current(&monitors, &windows),
            CurrentSource::Window(Some(1))
        );
        assert_eq!(
            SourceKind::Window { handle: 42, pid: 2 }.current(&monitors, &windows),
            CurrentSource::Window(None)
        );
    }

    #[test]
    fn a_lost_device_is_tried_again_after_the_pause_and_not_before() {
        let mut reconnect = Reconnect::new();
        let t0 = Instant::now();
        assert!(!reconnect.tick(true, t0), "the pause starts now");
        assert!(!reconnect.tick(true, t0 + RECONNECT_DELAY / 2));
        assert!(reconnect.tick(true, t0 + RECONNECT_DELAY), "time to try");
        // Still gone: another full pause before the next try.
        let t1 = t0 + RECONNECT_DELAY;
        assert!(!reconnect.tick(true, t1));
        assert!(reconnect.tick(true, t1 + RECONNECT_DELAY));
    }

    #[test]
    fn a_device_that_comes_back_clears_the_pause() {
        let mut reconnect = Reconnect::new();
        let t0 = Instant::now();
        reconnect.tick(true, t0);
        assert!(!reconnect.tick(false, t0 + RECONNECT_DELAY / 2));
        // Lost again later: the pause starts from scratch, not from the old one.
        let t1 = t0 + RECONNECT_DELAY;
        assert!(!reconnect.tick(true, t1));
        assert!(reconnect.tick(true, t1 + RECONNECT_DELAY));
    }

    #[test]
    fn a_window_is_found_by_title_exact_first_and_never_our_own() {
        let windows = [
            window(1, "Notes - draft", 10),
            window(2, "Notes", 11),
            window(3, "Dockview - Notes", 99),
        ];
        // Exact beats "contains", whatever the order.
        assert_eq!(
            find_window(&windows, "notes", 99).map(|w| w.handle),
            Some(2)
        );
        // Partial text finds the first one that contains it…
        assert_eq!(
            find_window(&windows, "draft", 99).map(|w| w.handle),
            Some(1)
        );
        // …but never a window of our own process.
        assert_eq!(find_window(&windows, "dockview", 99), None);
        assert_eq!(find_window(&windows, "missing", 99), None);
    }

    #[test]
    fn a_monitor_is_named_by_position_size_and_whether_it_is_primary() {
        assert_eq!(
            monitor_label(Lang::En, 0, &monitor(1, true)),
            "Screen 1 · 1920×1080 (primary)"
        );
        assert_eq!(
            monitor_label(Lang::En, 1, &monitor(2, false)),
            "Screen 2 · 1920×1080"
        );
    }

    #[test]
    fn status_maps_to_what_the_diagnosis_understands() {
        assert_eq!(
            video_state_of_capture(&CaptureStatus::Starting),
            VideoState::Connecting
        );
        assert_eq!(
            video_state_of_capture(&CaptureStatus::Stopped),
            VideoState::Running
        );
        assert_eq!(
            video_state_of_capture(&CaptureStatus::Failed("x".into())),
            VideoState::Lost
        );
        assert_eq!(
            video_state_of_screen(&ScreenStatus::Starting),
            VideoState::Connecting
        );
        assert_eq!(
            video_state_of_screen(&ScreenStatus::Failed("x".into())),
            VideoState::Lost
        );
    }

    #[test]
    fn the_status_light_follows_the_video_state() {
        assert_eq!(status_kind(VideoState::Running), StatusKind::Live);
        assert_eq!(status_kind(VideoState::Connecting), StatusKind::Connecting);
        assert_eq!(status_kind(VideoState::Lost), StatusKind::Problem);
        assert_eq!(status_kind(VideoState::NoDevice), StatusKind::Idle);
    }

    fn video(name: &str, link: &str) -> VideoDevice {
        VideoDevice {
            friendly_name: name.into(),
            symbolic_link: link.into(),
        }
    }

    fn audio(id: &str) -> AudioDevice {
        AudioDevice {
            friendly_name: id.to_uppercase(),
            device_id: id.into(),
        }
    }

    #[test]
    fn the_video_device_is_picked_by_name_then_last_used_then_first() {
        let devices = [video("UGREEN 25173", "a"), video("Integrated Camera", "b")];
        // The command line wins, matching part of the name in any case.
        assert_eq!(pick_video(&devices, Some("camera"), Some("a")), Some(1));
        // A name that matches nothing falls through to the last one used.
        assert_eq!(pick_video(&devices, Some("nothing"), Some("b")), Some(1));
        assert_eq!(pick_video(&devices, None, Some("b")), Some(1));
        // A last device that is gone falls through to the first.
        assert_eq!(pick_video(&devices, None, Some("gone")), Some(0));
        assert_eq!(pick_video(&devices, None, None), Some(0));
        assert_eq!(pick_video(&[], Some("x"), Some("y")), None);
    }

    #[test]
    fn the_audio_device_is_last_used_then_the_one_matching_the_video_then_first() {
        let devices = [audio("a"), audio("b"), audio("c")];
        assert_eq!(pick_audio(&devices, Some("c"), Some(1)), Some(2));
        assert_eq!(pick_audio(&devices, Some("gone"), Some(1)), Some(1));
        assert_eq!(pick_audio(&devices, None, None), Some(0));
        assert_eq!(pick_audio(&[], Some("a"), Some(0)), None);
        // A matched index past the end of the list is ignored.
        assert_eq!(pick_audio(&devices, None, Some(9)), Some(0));
    }

    #[test]
    fn the_startup_scene_follows_the_command_line_then_the_saved_one() {
        let scene = startup_scene;
        assert_eq!(
            scene(true, true, true, Some(2)),
            StartupScene::FromCommandLine
        );
        assert_eq!(scene(false, false, false, Some(2)), StartupScene::Saved(2));
        assert_eq!(scene(false, false, false, None), StartupScene::None);
        // Choosing another main source on the command line skips the saved scene.
        assert_eq!(scene(false, true, false, Some(2)), StartupScene::None);
        // So does the preview, which must look the same on every machine.
        assert_eq!(scene(false, false, true, Some(2)), StartupScene::None);
    }
}
