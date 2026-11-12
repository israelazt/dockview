//! Plain-language health checks over the numbers the app measures.
//!
//! Pure logic: the app feeds in a [`Snapshot`] and gets back [`Finding`]s,
//! which it then translates into the user's language.

/// Above this, an uncompressed mode needs a USB 3 port: USB 2.0 tops out
/// around 480 Mbps on paper and roughly 300 Mbps in practice.
pub const USB2_PRACTICAL_MBPS: f32 = 300.0;
/// Received frame rate below this share of the mode's rate is "low".
pub const LOW_RATE_SHARE: f32 = 0.85;
/// Dropped frames per second that count as a problem.
pub const DROPPED_PER_SEC_LIMIT: f32 = 1.0;
/// App CPU use (percent of one core) that counts as heavy.
pub const HIGH_CPU_PERCENT: f32 = 70.0;
/// GPU time per frame (share of the display's frame budget) that counts as heavy.
pub const GPU_BUDGET_SHARE: f32 = 0.5;

/// State of the video source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoState {
    /// No capture device is selected or found.
    NoDevice,
    Connecting,
    Running,
    /// The device was lost and the app is trying to reconnect.
    Lost,
}

/// Everything the diagnosis looks at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapshot {
    pub video: VideoState,
    /// Frame rate advertised by the selected device mode.
    pub mode_fps: Option<u32>,
    /// Data rate of the mode when sent uncompressed.
    pub mode_raw_mbps: Option<f32>,
    /// Frames per second actually received from the device.
    pub received_fps: f32,
    /// Frames per second replaced before being shown.
    pub dropped_per_sec: f32,
    /// Percent of one CPU core used by this app.
    pub cpu_core_percent: f32,
    /// GPU time spent per frame, when measured.
    pub gpu_ms: Option<f32>,
    /// Milliseconds between display refreshes, when known.
    pub frame_budget_ms: Option<f32>,
    /// `None` when no audio device is in use.
    pub audio_ok: Option<bool>,
}

/// A conclusion the interface can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finding {
    Healthy,
    NoVideo,
    VideoLost,
    /// The mode needs more USB bandwidth than a USB 2.0 port provides.
    NeedsUsb3,
    /// The device delivers fewer frames than its mode promises.
    CaptureBelowMode,
    DroppingFrames,
    HighCpu,
    SlowGpu,
    AudioLost,
    /// The app moved to a compressed mode because the USB port was too slow.
    SwitchedToCompressed,
}

/// Consecutive seconds of "USB too slow" before the app acts. Long enough to
/// ignore the first, noisy second after a start or a reconnect.
pub const FALLBACK_AFTER_SECS: u32 = 4;

/// Decides when to give up on an uncompressed mode.
///
/// Feed it one observation per second; it answers `true` once, after
/// [`FALLBACK_AFTER_SECS`] seconds in a row of the problem.
#[derive(Debug, Default)]
pub struct FallbackWatch {
    slow_seconds: u32,
}

impl FallbackWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// `usb_slow` is whether this second showed [`Finding::NeedsUsb3`];
    /// `eligible` is whether switching is allowed right now (automatic mode,
    /// video running, not already switched).
    pub fn observe(&mut self, usb_slow: bool, eligible: bool) -> bool {
        if !(usb_slow && eligible) {
            self.slow_seconds = 0;
            return false;
        }
        self.slow_seconds += 1;
        if self.slow_seconds >= FALLBACK_AFTER_SECS {
            self.slow_seconds = 0;
            return true;
        }
        false
    }

    /// Forgets any progress (after a device or mode change).
    pub fn reset(&mut self) {
        self.slow_seconds = 0;
    }
}

/// Whether the rate measured after switching to a compressed mode is a real
/// improvement over `before`. If not, the slowness had another cause (for
/// example a dim room for a webcam) and the switch is undone.
pub fn switch_helped(fps_before: f32, fps_after: f32) -> bool {
    fps_after >= fps_before * 1.25
}

/// Evaluates `s`. The result is never empty: it is `[Healthy]` when nothing
/// is wrong. Most important findings come first.
pub fn diagnose(s: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();

    match s.video {
        VideoState::NoDevice => findings.push(Finding::NoVideo),
        VideoState::Lost => findings.push(Finding::VideoLost),
        VideoState::Connecting => {}
        VideoState::Running => {
            let low_rate = s.mode_fps.is_some_and(|fps| {
                s.received_fps > 0.0 && s.received_fps < fps as f32 * LOW_RATE_SHARE
            });
            if low_rate {
                // A raw mode that cannot fit in USB 2.0 is the likely cause;
                // otherwise stay general.
                if s.mode_raw_mbps.is_some_and(|m| m > USB2_PRACTICAL_MBPS) {
                    findings.push(Finding::NeedsUsb3);
                } else {
                    findings.push(Finding::CaptureBelowMode);
                }
            }
            if s.dropped_per_sec >= DROPPED_PER_SEC_LIMIT {
                findings.push(Finding::DroppingFrames);
            }
            if s.cpu_core_percent > HIGH_CPU_PERCENT {
                findings.push(Finding::HighCpu);
            }
            if let (Some(gpu), Some(budget)) = (s.gpu_ms, s.frame_budget_ms) {
                if gpu > budget * GPU_BUDGET_SHARE {
                    findings.push(Finding::SlowGpu);
                }
            }
        }
    }
    if s.audio_ok == Some(false) {
        findings.push(Finding::AudioLost);
    }

    if findings.is_empty() {
        findings.push(Finding::Healthy);
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> Snapshot {
        Snapshot {
            video: VideoState::Running,
            mode_fps: Some(60),
            mode_raw_mbps: None,
            received_fps: 59.9,
            dropped_per_sec: 0.0,
            cpu_core_percent: 8.0,
            gpu_ms: Some(0.4),
            frame_budget_ms: Some(16.7),
            audio_ok: Some(true),
        }
    }

    #[test]
    fn the_watch_triggers_once_after_four_slow_seconds_in_a_row() {
        let mut watch = FallbackWatch::new();
        assert!(!watch.observe(true, true));
        assert!(!watch.observe(true, true));
        assert!(!watch.observe(true, true));
        assert!(watch.observe(true, true), "fourth slow second");
        assert!(!watch.observe(true, true), "counting restarts after firing");
    }

    #[test]
    fn a_good_second_or_ineligibility_resets_the_count() {
        let mut watch = FallbackWatch::new();
        for _ in 0..3 {
            watch.observe(true, true);
        }
        assert!(!watch.observe(false, true), "recovered");
        assert!(!watch.observe(true, true));
        for _ in 0..3 {
            assert!(!watch.observe(true, false), "not eligible: never fires");
        }
        let mut again = FallbackWatch::new();
        for _ in 0..3 {
            again.observe(true, true);
        }
        again.reset();
        assert!(!again.observe(true, true));
    }

    #[test]
    fn a_switch_only_counts_as_helpful_when_the_rate_clearly_improves() {
        assert!(switch_helped(20.0, 60.0));
        assert!(switch_helped(40.0, 50.0));
        assert!(
            !switch_helped(20.0, 21.0),
            "dim webcam: same rate either way"
        );
        assert!(!switch_helped(30.0, 30.0));
    }

    #[test]
    fn a_healthy_session_reports_only_healthy() {
        assert_eq!(diagnose(&healthy()), vec![Finding::Healthy]);
    }

    #[test]
    fn no_device_and_lost_device_are_reported_first() {
        let none = Snapshot {
            video: VideoState::NoDevice,
            ..healthy()
        };
        assert_eq!(diagnose(&none), vec![Finding::NoVideo]);
        let lost = Snapshot {
            video: VideoState::Lost,
            ..healthy()
        };
        assert_eq!(diagnose(&lost), vec![Finding::VideoLost]);
    }

    #[test]
    fn connecting_is_not_a_problem_yet() {
        let s = Snapshot {
            video: VideoState::Connecting,
            received_fps: 0.0,
            ..healthy()
        };
        assert_eq!(diagnose(&s), vec![Finding::Healthy]);
    }

    #[test]
    fn a_device_below_its_mode_rate_is_flagged() {
        // The webcam case: a 30 fps mode delivering about 20 fps.
        let s = Snapshot {
            mode_fps: Some(30),
            received_fps: 20.0,
            ..healthy()
        };
        assert_eq!(diagnose(&s), vec![Finding::CaptureBelowMode]);
    }

    #[test]
    fn a_heavy_raw_mode_below_its_rate_points_at_usb3() {
        let s = Snapshot {
            mode_fps: Some(30),
            mode_raw_mbps: Some(746.0),
            received_fps: 20.0,
            ..healthy()
        };
        assert_eq!(diagnose(&s), vec![Finding::NeedsUsb3]);
    }

    #[test]
    fn a_light_raw_mode_is_not_blamed_on_usb() {
        let s = Snapshot {
            mode_fps: Some(30),
            mode_raw_mbps: Some(147.0),
            received_fps: 20.0,
            ..healthy()
        };
        assert_eq!(diagnose(&s), vec![Finding::CaptureBelowMode]);
    }

    #[test]
    fn rate_just_under_the_mode_is_tolerated() {
        let s = Snapshot {
            mode_fps: Some(60),
            received_fps: 54.0,
            ..healthy()
        };
        assert_eq!(diagnose(&s), vec![Finding::Healthy]);
    }

    #[test]
    fn dropped_frames_cpu_and_gpu_are_independent_findings() {
        let s = Snapshot {
            dropped_per_sec: 3.0,
            cpu_core_percent: 90.0,
            gpu_ms: Some(10.0),
            ..healthy()
        };
        assert_eq!(
            diagnose(&s),
            vec![Finding::DroppingFrames, Finding::HighCpu, Finding::SlowGpu]
        );
    }

    #[test]
    fn unknown_measurements_never_raise_findings() {
        let s = Snapshot {
            gpu_ms: None,
            frame_budget_ms: None,
            mode_fps: None,
            ..healthy()
        };
        assert_eq!(diagnose(&s), vec![Finding::Healthy]);
    }

    #[test]
    fn a_lost_audio_device_is_reported_even_when_video_is_fine() {
        let s = Snapshot {
            audio_ok: Some(false),
            ..healthy()
        };
        assert_eq!(diagnose(&s), vec![Finding::AudioLost]);
        let none = Snapshot {
            audio_ok: None,
            ..healthy()
        };
        assert_eq!(diagnose(&none), vec![Finding::Healthy]);
    }
}
