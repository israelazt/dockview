//! How well a stream is doing, in one word. The engine already counts the
//! pictures it sent and the ones it threw away; this turns those counts into
//! a verdict the user can act on ("lower the bitrate"), judged over short
//! windows so that a bad minute does not follow the stream for the whole
//! evening.

use std::time::{Duration, Instant};

use crate::broadcast::{StreamState, StreamStats};

/// How long each judgement looks at.
pub const WINDOW: Duration = Duration::from_secs(5);
/// Fewer pictures than this in a window say nothing yet.
const MIN_FRAMES: u64 = 30;
/// Share of pictures lost from which the connection is weak / poor.
const WEAK_LOSS: f32 = 0.01;
const POOR_LOSS: f32 = 0.05;
/// Below this share of the chosen bitrate actually sent, the connection is weak.
const WEAK_BITRATE: f32 = 0.5;

/// The verdict on a stream's connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamHealth {
    /// Nothing is being lost.
    #[default]
    Good,
    /// Some pictures are lost or less is sent than asked for.
    Weak,
    /// Many pictures are lost: the viewers see stutter.
    Poor,
}

/// The verdict for a window: `dropped` and `sent` are the pictures thrown
/// away and sent during it; `sent_kbps` is what goes out now and `target_kbps`
/// what was asked for.
pub fn judge(dropped: u64, sent: u64, sent_kbps: u32, target_kbps: u32) -> StreamHealth {
    let total = dropped + sent;
    if total < MIN_FRAMES {
        return StreamHealth::Good;
    }
    let loss = dropped as f32 / total as f32;
    if loss >= POOR_LOSS {
        StreamHealth::Poor
    } else if loss >= WEAK_LOSS
        || (target_kbps > 0 && (sent_kbps as f32) < target_kbps as f32 * WEAK_BITRATE)
    {
        StreamHealth::Weak
    } else {
        StreamHealth::Good
    }
}

/// Watches a stream window by window.
#[derive(Debug, Default)]
pub struct HealthMonitor {
    /// When the current window began and the counters at that moment.
    window: Option<(Instant, u64, u64)>,
    health: StreamHealth,
}

impl HealthMonitor {
    /// The last verdict.
    pub fn health(&self) -> StreamHealth {
        self.health
    }

    /// Looks at the stream at `now`. Anything but a live stream starts over
    /// as good. The verdict changes only when a window closes.
    pub fn update(&mut self, now: Instant, stats: &StreamStats, target_kbps: u32) -> StreamHealth {
        if stats.state != StreamState::Live {
            *self = Self::default();
            return self.health;
        }
        let (start, dropped0, sent0) =
            self.window
                .get_or_insert((now, stats.dropped_frames, stats.sent_frames));
        if now.saturating_duration_since(*start) >= WINDOW {
            self.health = judge(
                stats.dropped_frames.saturating_sub(*dropped0),
                stats.sent_frames.saturating_sub(*sent0),
                stats.bitrate_bps / 1000,
                target_kbps,
            );
            self.window = Some((now, stats.dropped_frames, stats.sent_frames));
        }
        self.health
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(sent: u64, dropped: u64, kbps: u32) -> StreamStats {
        StreamStats {
            state: StreamState::Live,
            bitrate_bps: kbps * 1000,
            sent_frames: sent,
            dropped_frames: dropped,
            reconnects: 0,
        }
    }

    #[test]
    fn losing_pictures_makes_it_weak_and_then_poor() {
        assert_eq!(judge(0, 300, 4500, 4500), StreamHealth::Good);
        assert_eq!(judge(4, 296, 4500, 4500), StreamHealth::Weak);
        assert_eq!(judge(15, 285, 4500, 4500), StreamHealth::Poor);
    }

    #[test]
    fn sending_far_less_than_asked_is_weak_but_never_poor() {
        assert_eq!(judge(0, 300, 1500, 4500), StreamHealth::Weak);
        assert_eq!(judge(0, 300, 3000, 4500), StreamHealth::Good);
        assert_eq!(
            judge(0, 300, 0, 0),
            StreamHealth::Good,
            "no target, no test"
        );
    }

    #[test]
    fn a_window_with_almost_no_pictures_says_nothing() {
        assert_eq!(judge(5, 5, 100, 4500), StreamHealth::Good);
    }

    #[test]
    fn the_verdict_changes_only_when_a_window_closes() {
        let mut m = HealthMonitor::default();
        let t0 = Instant::now();
        assert_eq!(m.update(t0, &live(0, 0, 4500), 4500), StreamHealth::Good);
        // Mid-window: heavy loss is not judged yet.
        let mid = t0 + Duration::from_secs(2);
        assert_eq!(m.update(mid, &live(60, 60, 4500), 4500), StreamHealth::Good);
        let end = t0 + WINDOW;
        assert_eq!(
            m.update(end, &live(150, 150, 4500), 4500),
            StreamHealth::Poor
        );
        assert_eq!(m.health(), StreamHealth::Poor);
    }

    #[test]
    fn each_window_counts_only_its_own_pictures_so_it_recovers() {
        let mut m = HealthMonitor::default();
        let t0 = Instant::now();
        m.update(t0, &live(0, 0, 4500), 4500);
        m.update(t0 + WINDOW, &live(150, 150, 4500), 4500);
        assert_eq!(m.health(), StreamHealth::Poor);
        // The next window has no losses at all, though the total still does.
        let verdict = m.update(t0 + WINDOW * 2, &live(450, 150, 4500), 4500);
        assert_eq!(verdict, StreamHealth::Good);
    }

    #[test]
    fn anything_but_live_starts_over() {
        let mut m = HealthMonitor::default();
        let t0 = Instant::now();
        m.update(t0, &live(0, 0, 4500), 4500);
        m.update(t0 + WINDOW, &live(150, 150, 4500), 4500);
        let reconnecting = StreamStats {
            state: StreamState::Reconnecting,
            ..live(150, 150, 0)
        };
        assert_eq!(
            m.update(t0 + WINDOW * 2, &reconnecting, 4500),
            StreamHealth::Good
        );
        // Back live: a fresh window, not judged on the old counters.
        let back = t0 + WINDOW * 3;
        assert_eq!(
            m.update(back, &live(150, 150, 4500), 4500),
            StreamHealth::Good
        );
        assert_eq!(
            m.update(back + WINDOW, &live(450, 150, 4500), 4500),
            StreamHealth::Good
        );
    }
}
