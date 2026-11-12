//! Pure selection logic for choosing the best native stream type exposed by a
//! capture device. Hardware-independent so it can be unit tested.

use std::fmt;

/// How the native stream format is chosen when a device exposes several.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureProfile {
    /// Highest FPS, then highest resolution; any format (even MJPEG).
    LowLatency,
    /// Uncompressed preferred; largest pixel throughput (width × height × fps).
    #[default]
    Balanced,
    /// Uncompressed preferred; largest resolution first, FPS as tiebreaker.
    Quality,
}

/// A single native media type exposed by the capture device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    /// Index of the media type in the device's native type list.
    pub type_index: u32,
    /// Human-readable format name, e.g. `YUY2`, `NV12`, `MJPEG`.
    pub format_name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// `true` for compressed formats such as MJPEG, which need decoding.
    pub is_compressed: bool,
}

impl StreamInfo {
    fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    fn is_1080p60(&self) -> bool {
        self.width == 1920 && self.height == 1080 && self.fps >= 55
    }
}

impl fmt::Display for StreamInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} · {}×{} · {}fps",
            self.format_name, self.width, self.height, self.fps
        )
    }
}

/// Picks the best stream for `profile`, or `None` if `types` is empty.
///
/// 1920×1080 at ≥55 fps is always preferred when the device offers it; the
/// full list is only considered when it does not. Ties resolve to the entry
/// that appears first, matching the original C# implementation.
pub fn select_best(types: &[StreamInfo], profile: CaptureProfile) -> Option<&StreamInfo> {
    if types.is_empty() {
        return None;
    }

    let target: Vec<&StreamInfo> = types.iter().filter(|t| t.is_1080p60()).collect();
    let pool: Vec<&StreamInfo> = if target.is_empty() {
        types.iter().collect()
    } else {
        target
    };

    match profile {
        CaptureProfile::LowLatency => first_max_by_key(pool, |t| (u64::from(t.fps), t.pixels())),
        CaptureProfile::Quality => prefer_uncompressed(pool, |t| (t.pixels(), u64::from(t.fps))),
        CaptureProfile::Balanced => {
            prefer_uncompressed(pool, |t| (t.pixels() * u64::from(t.fps), 0))
        }
    }
}

/// Restricts `pool` to uncompressed streams when any exist, then picks the
/// maximum by `order`.
fn prefer_uncompressed(
    pool: Vec<&StreamInfo>,
    order: impl Fn(&StreamInfo) -> (u64, u64),
) -> Option<&StreamInfo> {
    let uncompressed: Vec<&StreamInfo> =
        pool.iter().copied().filter(|t| !t.is_compressed).collect();
    let pool = if uncompressed.is_empty() {
        pool
    } else {
        uncompressed
    };
    first_max_by_key(pool, order)
}

/// Like `Iterator::max_by_key`, but keeps the *first* maximal element on ties
/// (the std version returns the last).
fn first_max_by_key<K: Ord>(
    items: Vec<&StreamInfo>,
    key: impl Fn(&StreamInfo) -> K,
) -> Option<&StreamInfo> {
    let mut best: Option<(&StreamInfo, K)> = None;
    for item in items {
        let k = key(item);
        match &best {
            Some((_, best_key)) if k <= *best_key => {}
            _ => best = Some((item, k)),
        }
    }
    best.map(|(item, _)| item)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(width: u32, height: u32, fps: u32, compressed: bool, fmt: &str) -> StreamInfo {
        StreamInfo {
            type_index: 0,
            format_name: fmt.to_string(),
            width,
            height,
            fps,
            is_compressed: compressed,
        }
    }

    fn nv12_1440p35() -> StreamInfo {
        make(2560, 1440, 35, false, "NV12")
    }
    fn yuy2_1080p60() -> StreamInfo {
        make(1920, 1080, 60, false, "YUY2")
    }
    fn mjpeg_1080p60() -> StreamInfo {
        make(1920, 1080, 60, true, "MJPEG")
    }
    fn yuy2_1080p30() -> StreamInfo {
        make(1920, 1080, 30, false, "YUY2")
    }
    fn yuy2_720p60() -> StreamInfo {
        make(1280, 720, 60, false, "YUY2")
    }

    const ALL_PROFILES: [CaptureProfile; 3] = [
        CaptureProfile::LowLatency,
        CaptureProfile::Balanced,
        CaptureProfile::Quality,
    ];

    #[test]
    fn empty_list_returns_none() {
        assert_eq!(select_best(&[], CaptureProfile::Balanced), None);
    }

    #[test]
    fn prefers_1080p60_over_higher_resolution_for_every_profile() {
        let types = [nv12_1440p35(), yuy2_1080p60()];
        for profile in ALL_PROFILES {
            assert_eq!(select_best(&types, profile), Some(&types[1]), "{profile:?}");
        }
    }

    #[test]
    fn without_1080p60_falls_back_to_general_logic() {
        // 2560×1440×35 > 1280×720×60, so Balanced picks the 1440p mode.
        let types = [nv12_1440p35(), yuy2_720p60()];
        assert_eq!(
            select_best(&types, CaptureProfile::Balanced),
            Some(&types[0])
        );
    }

    #[test]
    fn accepts_fps_of_55_and_above_as_60hz_class() {
        let types = [make(1920, 1080, 55, false, "YUY2"), nv12_1440p35()];
        assert_eq!(
            select_best(&types, CaptureProfile::Balanced),
            Some(&types[0])
        );
    }

    #[test]
    fn rejects_1080p_below_55_fps_as_target() {
        let types = [make(1920, 1080, 54, false, "YUY2"), nv12_1440p35()];
        // Pool is the full list, so the larger throughput mode wins.
        assert_eq!(
            select_best(&types, CaptureProfile::Balanced),
            Some(&types[1])
        );
    }

    #[test]
    fn balanced_prefers_uncompressed_over_mjpeg_at_same_mode() {
        let types = [mjpeg_1080p60(), yuy2_1080p60()];
        assert_eq!(
            select_best(&types, CaptureProfile::Balanced),
            Some(&types[1])
        );
    }

    #[test]
    fn balanced_with_only_mjpeg_returns_it() {
        let types = [mjpeg_1080p60()];
        assert_eq!(
            select_best(&types, CaptureProfile::Balanced),
            Some(&types[0])
        );
    }

    #[test]
    fn quality_uses_mjpeg_when_the_1080p60_pool_has_no_uncompressed() {
        // Pool is {Mjpeg_1080p60}; the uncompressed 1080p30 is outside it.
        let types = [mjpeg_1080p60(), yuy2_1080p30()];
        assert_eq!(
            select_best(&types, CaptureProfile::Quality),
            Some(&types[0])
        );
    }

    #[test]
    fn quality_uncompressed_in_pool_wins_over_compressed() {
        let types = [mjpeg_1080p60(), yuy2_1080p60()];
        assert_eq!(
            select_best(&types, CaptureProfile::Quality),
            Some(&types[1])
        );
    }

    #[test]
    fn low_latency_picks_highest_fps_in_pool() {
        let types = [mjpeg_1080p60(), yuy2_1080p60()];
        let best = select_best(&types, CaptureProfile::LowLatency).unwrap();
        assert_eq!(best.fps, 60);
    }

    #[test]
    fn low_latency_without_pool_picks_highest_fps_overall() {
        let types = [yuy2_1080p30(), yuy2_720p60()];
        assert_eq!(
            select_best(&types, CaptureProfile::LowLatency),
            Some(&types[1])
        );
    }

    #[test]
    fn ties_resolve_to_the_first_entry() {
        let a = make(1920, 1080, 60, false, "YUY2");
        let mut b = a.clone();
        b.type_index = 1;
        let types = [a, b];
        for profile in ALL_PROFILES {
            assert_eq!(select_best(&types, profile), Some(&types[0]), "{profile:?}");
        }
    }

    #[test]
    fn display_label_matches_csharp_format() {
        assert_eq!(yuy2_1080p60().to_string(), "YUY2 · 1920×1080 · 60fps");
    }
}
