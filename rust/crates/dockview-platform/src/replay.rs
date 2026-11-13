//! The replay buffer: a ring of compressed video frames.
//!
//! Pure data structure (no Windows APIs), so the cutting rules can be tested
//! thoroughly. The buffer always starts at a keyframe, because a clip can only
//! begin where decoding can begin.

use std::collections::VecDeque;

use crate::encode::EncodedFrame;

/// 100 ns units per second.
const TICKS_PER_SECOND: f64 = 10_000_000.0;

/// Compressed frames of the most recent footage.
#[derive(Debug)]
pub struct ReplayRing {
    frames: VecDeque<EncodedFrame>,
    /// How much footage to keep, in 100 ns units.
    keep: i64,
    bytes: usize,
}

impl ReplayRing {
    /// Keeps at least `seconds` of footage.
    pub fn new(seconds: f64) -> Self {
        Self {
            frames: VecDeque::new(),
            keep: (seconds.max(0.0) * TICKS_PER_SECOND) as i64,
            bytes: 0,
        }
    }

    /// Changes how much footage is kept (applied at the next [`push`]).
    pub fn set_keep_seconds(&mut self, seconds: f64) {
        self.keep = (seconds.max(0.0) * TICKS_PER_SECOND) as i64;
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Compressed size currently held, in bytes.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Seconds of footage between the first frame and the end of the last.
    pub fn duration_seconds(&self) -> f64 {
        match (self.frames.front(), self.frames.back()) {
            (Some(first), Some(last)) => {
                (last.timestamp + last.duration - first.timestamp) as f64 / TICKS_PER_SECOND
            }
            _ => 0.0,
        }
    }

    /// Adds the newest frame and discards footage that is no longer needed.
    ///
    /// Frames before the very first keyframe are ignored, so the buffer never
    /// starts on something that cannot be decoded.
    pub fn push(&mut self, frame: EncodedFrame) {
        if self.frames.is_empty() && !frame.keyframe {
            return;
        }
        self.bytes += frame.data.len();
        self.frames.push_back(frame);
        self.trim();
    }

    /// Drops whole groups of pictures from the front while the remaining
    /// footage is still at least `keep` long.
    fn trim(&mut self) {
        let Some(newest_end) = self.frames.back().map(|f| f.timestamp + f.duration) else {
            return;
        };
        loop {
            // The next keyframe after the first frame marks the end of the
            // oldest group of pictures.
            let Some(next) = self
                .frames
                .iter()
                .skip(1)
                .position(|f| f.keyframe)
                .map(|i| i + 1)
            else {
                return;
            };
            if newest_end - self.frames[next].timestamp < self.keep {
                return;
            }
            for _ in 0..next {
                if let Some(dropped) = self.frames.pop_front() {
                    self.bytes -= dropped.data.len();
                }
            }
        }
    }

    /// The frames of a clip covering *at least* the last `seconds`.
    ///
    /// It starts at the latest keyframe at or before that moment, so it can be
    /// a little longer than requested (up to one keyframe interval) but never
    /// shorter, unless the buffer simply holds less. Empty when the buffer is.
    pub fn clip(&self, seconds: f64) -> Vec<&EncodedFrame> {
        let Some(newest_end) = self.frames.back().map(|f| f.timestamp + f.duration) else {
            return Vec::new();
        };
        let wanted_start = newest_end - (seconds.max(0.0) * TICKS_PER_SECOND) as i64;
        let start = self
            .frames
            .iter()
            .rposition(|f| f.keyframe && f.timestamp <= wanted_start)
            .unwrap_or(0);
        self.frames.iter().skip(start).collect()
    }

    /// Forgets everything (for example when the video source changes).
    pub fn clear(&mut self) {
        self.frames.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: i64 = 60;
    const FRAME: i64 = 10_000_000 / FPS;

    /// A frame `index` frames into the stream; every `gop` frames is a keyframe.
    fn frame(index: i64, gop: i64) -> EncodedFrame {
        EncodedFrame {
            data: vec![0u8; 100],
            timestamp: index * FRAME,
            duration: FRAME,
            keyframe: index % gop == 0,
        }
    }

    fn filled(seconds_kept: f64, total_seconds: i64) -> ReplayRing {
        let mut ring = ReplayRing::new(seconds_kept);
        for i in 0..total_seconds * FPS {
            ring.push(frame(i, FPS));
        }
        ring
    }

    fn seconds(frames: &[&EncodedFrame]) -> f64 {
        let (first, last) = (frames[0], frames[frames.len() - 1]);
        (last.timestamp + last.duration - first.timestamp) as f64 / TICKS_PER_SECOND
    }

    #[test]
    fn an_empty_ring_has_nothing() {
        let ring = ReplayRing::new(5.0);
        assert!(ring.is_empty() && ring.clip(3.0).is_empty());
        assert_eq!((ring.len(), ring.bytes()), (0, 0));
        assert_eq!(ring.duration_seconds(), 0.0);
    }

    #[test]
    fn it_keeps_at_least_the_requested_footage_and_not_much_more() {
        let ring = filled(4.0, 10);
        let held = ring.duration_seconds();
        assert!(held >= 4.0, "held {held}");
        assert!(
            held < 5.0 + 1e-6,
            "held {held}: more than one keyframe interval extra"
        );
    }

    #[test]
    fn it_always_starts_on_a_keyframe() {
        let ring = filled(3.0, 12);
        assert!(ring.clip(100.0)[0].keyframe);
    }

    #[test]
    fn byte_accounting_follows_what_is_held() {
        let ring = filled(2.0, 8);
        assert_eq!(ring.bytes(), ring.len() * 100);
    }

    #[test]
    fn frames_before_the_first_keyframe_are_ignored() {
        let mut ring = ReplayRing::new(5.0);
        ring.push(frame(1, FPS)); // not a keyframe
        ring.push(frame(2, FPS));
        assert!(ring.is_empty());
        ring.push(frame(60, FPS));
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn a_clip_is_never_shorter_than_requested() {
        let ring = filled(10.0, 10);
        for wanted in [0.25, 0.5, 1.0, 1.5, 2.0, 3.3, 5.0, 9.0] {
            let clip = ring.clip(wanted);
            assert!(clip[0].keyframe, "starts on a keyframe for {wanted}");
            assert!(
                seconds(&clip) >= wanted - 1e-6,
                "{wanted}: got {}",
                seconds(&clip)
            );
            assert!(
                seconds(&clip) < wanted + 1.0 + 1e-6,
                "{wanted}: got {}",
                seconds(&clip)
            );
        }
    }

    #[test]
    fn asking_for_more_than_is_held_returns_everything() {
        let ring = filled(3.0, 10);
        let clip = ring.clip(60.0);
        assert_eq!(clip.len(), ring.len());
    }

    #[test]
    fn the_clip_ends_at_the_newest_frame() {
        let ring = filled(5.0, 10);
        let clip = ring.clip(2.0);
        assert_eq!(clip.last().unwrap().timestamp, 10 * FPS * FRAME - FRAME);
    }

    #[test]
    fn a_very_short_clip_still_starts_at_a_keyframe_before_the_moment() {
        let ring = filled(5.0, 10);
        // 0.1 s: the latest keyframe is up to 1 s old, so the clip is ≥ 0.1 s.
        let clip = ring.clip(0.1);
        assert!(clip[0].keyframe);
        assert!(seconds(&clip) >= 0.1);
    }

    #[test]
    fn growing_the_window_keeps_the_footage_and_shrinking_it_trims() {
        let mut ring = filled(4.0, 8);
        let before = ring.duration_seconds();
        ring.set_keep_seconds(10.0);
        for i in 8 * FPS..9 * FPS {
            ring.push(frame(i, FPS));
        }
        assert!(ring.duration_seconds() > before, "nothing was discarded");
        ring.set_keep_seconds(2.0);
        ring.push(frame(9 * FPS, FPS));
        assert!(ring.duration_seconds() < 4.0, "trimmed down to ~2s");
        assert!(ring.duration_seconds() >= 2.0);
    }

    #[test]
    fn clearing_empties_the_ring_and_the_byte_count() {
        let mut ring = filled(3.0, 5);
        ring.clear();
        assert!(ring.is_empty());
        assert_eq!(ring.bytes(), 0);
        ring.push(frame(0, FPS));
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn irregular_keyframe_spacing_is_handled() {
        // Keyframes at 0, 90 and 200 frames: GOPs of unequal length.
        let mut ring = ReplayRing::new(2.0);
        for i in 0..400 {
            let mut f = frame(i, 1_000_000);
            f.keyframe = matches!(i, 0 | 90 | 200 | 300);
            ring.push(f);
        }
        assert!(ring.clip(100.0)[0].keyframe);
        assert!(ring.duration_seconds() >= 2.0);
    }
}
