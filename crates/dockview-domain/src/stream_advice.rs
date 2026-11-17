//! What each streaming platform recommends, so the Stream section can say
//! whether the chosen bitrate suits it. The numbers are the platforms' public
//! guidance for a 1080p picture; they change now and then, so this is advice
//! shown to the user, never a limit enforced on them.

/// The platforms with a ready-made address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Twitch,
    YouTube,
    Kick,
}

impl Platform {
    /// Every platform, in the order the Stream section offers them.
    pub const ALL: [Platform; 3] = [Platform::Twitch, Platform::YouTube, Platform::Kick];

    /// The platform an ingest address belongs to, if it is one of them.
    pub fn of_address(address: &str) -> Option<Platform> {
        let a = address.to_ascii_lowercase();
        if a.contains("twitch") {
            Some(Platform::Twitch)
        } else if a.contains("youtube") {
            Some(Platform::YouTube)
        } else if a.contains("kick") || a.contains("live-video.net") {
            Some(Platform::Kick)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Platform::Twitch => "Twitch",
            Platform::YouTube => "YouTube",
            Platform::Kick => "Kick",
        }
    }

    /// The recommended video bitrate range, in kilobits per second, for a
    /// 1080p picture at `fps`.
    pub fn recommended_kbps(self, fps: u32) -> (u32, u32) {
        let smooth = fps > 30;
        match (self, smooth) {
            (Platform::Twitch, false) => (3000, 6000),
            (Platform::Twitch, true) => (4500, 6000),
            (Platform::YouTube, false) => (3000, 6000),
            (Platform::YouTube, true) => (4500, 9000),
            (Platform::Kick, false) => (3000, 8000),
            (Platform::Kick, true) => (4500, 8000),
        }
    }
}

/// How a bitrate compares with what a platform recommends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Low,
    Fine,
    High,
}

/// How `kbps` compares with the range recommended for `platform` at `fps`.
pub fn fit(platform: Platform, fps: u32, kbps: u32) -> Fit {
    let (min, max) = platform.recommended_kbps(fps);
    if kbps < min {
        Fit::Low
    } else if kbps > max {
        Fit::High
    } else {
        Fit::Fine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_belongs_to_its_platform() {
        assert_eq!(
            Platform::of_address("rtmps://live.twitch.tv:443/app"),
            Some(Platform::Twitch)
        );
        assert_eq!(
            Platform::of_address("RTMPS://a.rtmps.youtube.com:443/live2"),
            Some(Platform::YouTube)
        );
        assert_eq!(
            Platform::of_address("rtmps://x.global-contribute.live-video.net:443/app"),
            Some(Platform::Kick)
        );
        assert_eq!(Platform::of_address("rtmp://my.server/app"), None);
    }

    #[test]
    fn the_range_is_wider_at_sixty_pictures_a_second_and_never_empty() {
        for p in Platform::ALL {
            let (lo30, hi30) = p.recommended_kbps(30);
            let (lo60, hi60) = p.recommended_kbps(60);
            assert!(lo30 < hi30 && lo60 < hi60, "{p:?}");
            assert!(lo60 >= lo30 && hi60 >= hi30, "{p:?}");
        }
    }

    #[test]
    fn a_bitrate_is_low_fine_or_high() {
        assert_eq!(fit(Platform::YouTube, 60, 3000), Fit::Low);
        assert_eq!(fit(Platform::YouTube, 60, 6000), Fit::Fine);
        assert_eq!(fit(Platform::Twitch, 30, 8000), Fit::High);
        assert_eq!(
            fit(Platform::Twitch, 60, 6000),
            Fit::Fine,
            "the edge is fine"
        );
    }
}
