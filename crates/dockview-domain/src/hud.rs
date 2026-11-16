//! The readouts over the picture in the viewer: what counts as a low latency,
//! and the words for it. Plain rules, tested without a window.

use crate::i18n::Key;

/// How good the app's own latency is (time from a picture reaching the app to
/// being handed to the screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatencyTier {
    UltraLow,
    Low,
    Medium,
    High,
}

/// Up to here (milliseconds, not including) is ultra low.
pub const ULTRA_LOW_MS: f32 = 3.0;
pub const LOW_MS: f32 = 8.0;
pub const MEDIUM_MS: f32 = 20.0;

/// The tier of a latency in milliseconds.
pub fn latency_tier(ms: f32) -> LatencyTier {
    if ms < ULTRA_LOW_MS {
        LatencyTier::UltraLow
    } else if ms < LOW_MS {
        LatencyTier::Low
    } else if ms < MEDIUM_MS {
        LatencyTier::Medium
    } else {
        LatencyTier::High
    }
}

impl LatencyTier {
    pub fn label(self) -> Key {
        match self {
            Self::UltraLow => Key::HudUltraLow,
            Self::Low => Key::HudLow,
            Self::Medium => Key::HudMedium,
            Self::High => Key::HudHigh,
        }
    }

    /// Whether the user should look at it.
    pub fn is_bad(self) -> bool {
        self == Self::High
    }
}

/// The short name of the pixel format in a mode like
/// `NV12 · 1920×1080 · 30fps`: its first word. `None` when there is no mode.
pub fn format_of(device_mode: &str) -> Option<&str> {
    device_mode
        .split(['·', ' '])
        .map(str::trim)
        .find(|w| !w.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_goes_from_ultra_low_to_high_at_the_limits() {
        assert_eq!(latency_tier(0.0), LatencyTier::UltraLow);
        assert_eq!(latency_tier(2.99), LatencyTier::UltraLow);
        assert_eq!(latency_tier(3.0), LatencyTier::Low);
        assert_eq!(latency_tier(7.9), LatencyTier::Low);
        assert_eq!(latency_tier(8.0), LatencyTier::Medium);
        assert_eq!(latency_tier(19.9), LatencyTier::Medium);
        assert_eq!(latency_tier(20.0), LatencyTier::High);
        assert!(LatencyTier::High.is_bad() && !LatencyTier::Medium.is_bad());
    }

    #[test]
    fn the_format_is_the_first_word_of_the_mode() {
        assert_eq!(format_of("NV12 · 1920×1080 · 30fps"), Some("NV12"));
        assert_eq!(format_of("MJPEG · 1280×720 · 60fps"), Some("MJPEG"));
        assert_eq!(format_of("  "), None);
        assert_eq!(format_of(""), None);
    }
}
