//! Platform-independent audio buffering: the jitter/sync ring and the drift
//! correction policy. Interleaved `f32` samples, fixed format.

use std::collections::VecDeque;

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// Bytes per interleaved frame (`f32` × channels).
pub const BYTES_PER_FRAME: usize = CHANNELS * std::mem::size_of::<f32>();

/// Buffer depth kept at zero offset. Positive offsets add to it; negative
/// offsets remove from it, down to [`MIN_TARGET_MS`].
pub const BASE_TARGET_MS: i32 = 60;
/// The buffer never targets less than this, whatever the offset.
pub const MIN_TARGET_MS: i32 = 15;
/// Upper bound on buffered audio; the oldest samples are dropped beyond it.
pub const MAX_BUFFER_MS: i32 = 500;
/// Fill deviations smaller than this are ignored.
pub const DEAD_ZONE_MS: i32 = 15;
/// Deviations larger than this are corrected at once (e.g. the user changed
/// the offset) instead of being nudged.
pub const SNAP_MS: i32 = 50;
/// Size of one gentle drift correction step.
pub const NUDGE_MS: i32 = 1;

/// Converts milliseconds to a whole number of frames at [`SAMPLE_RATE`].
pub fn ms_to_frames(ms: i32) -> usize {
    (i64::from(ms.max(0)) * i64::from(SAMPLE_RATE) / 1000) as usize
}

/// Frames the ring should hold for a given user sync offset.
pub fn target_frames(offset_ms: i32) -> usize {
    ms_to_frames((BASE_TARGET_MS + offset_ms).max(MIN_TARGET_MS))
}

/// How to bring the buffer fill back towards its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Correction {
    None,
    /// Drop this many frames (the buffer is too full: audio runs late).
    Discard(usize),
    /// Add this many frames of silence (the buffer is too empty).
    Inject(usize),
}

/// Decides the correction for the current `fill` versus `target` (in frames).
///
/// Small deviations are ignored (dead zone), moderate ones are nudged by a
/// millisecond at a time, and large ones are snapped to the target.
pub fn plan_correction(fill: usize, target: usize) -> Correction {
    let delta = fill as i64 - target as i64;
    let magnitude = delta.unsigned_abs() as usize;

    if magnitude < ms_to_frames(DEAD_ZONE_MS) {
        return Correction::None;
    }
    let amount = if magnitude >= ms_to_frames(SNAP_MS) {
        magnitude
    } else {
        ms_to_frames(NUDGE_MS).min(magnitude)
    };
    if delta > 0 {
        Correction::Discard(amount)
    } else {
        Correction::Inject(amount)
    }
}

/// FIFO of interleaved samples between the capture and render threads.
#[derive(Debug)]
pub struct AudioRing {
    samples: VecDeque<f32>,
    max_samples: usize,
}

impl Default for AudioRing {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioRing {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            max_samples: ms_to_frames(MAX_BUFFER_MS) * CHANNELS,
        }
    }

    /// Whole frames currently buffered.
    pub fn frames(&self) -> usize {
        self.samples.len() / CHANNELS
    }

    /// Appends interleaved samples, dropping the oldest if over capacity.
    pub fn push(&mut self, interleaved: &[f32]) {
        self.samples.extend(interleaved);
        let excess = self.samples.len().saturating_sub(self.max_samples);
        if excess > 0 {
            // Keep channel alignment: drop whole frames only.
            self.samples.drain(..excess.div_ceil(CHANNELS) * CHANNELS);
        }
    }

    /// Moves up to `out.len() / CHANNELS` frames into `out`, scaled by
    /// `gain`. Returns the number of frames written; the rest of `out` is
    /// left untouched.
    pub fn pop_into(&mut self, out: &mut [f32], gain: f32) -> usize {
        let frames = (out.len() / CHANNELS).min(self.frames());
        for (slot, sample) in out.iter_mut().zip(self.samples.drain(..frames * CHANNELS)) {
            *slot = sample * gain;
        }
        frames
    }

    /// Drops up to `frames` of the oldest frames.
    pub fn discard(&mut self, frames: usize) {
        let samples = (frames * CHANNELS).min(self.samples.len());
        self.samples.drain(..samples);
    }

    /// Appends `frames` of silence.
    pub fn inject_silence(&mut self, frames: usize) {
        self.samples
            .extend(std::iter::repeat_n(0.0, frames * CHANNELS));
    }

    /// Applies a [`Correction`].
    pub fn apply(&mut self, correction: Correction) {
        match correction {
            Correction::None => {}
            Correction::Discard(n) => self.discard(n),
            Correction::Inject(n) => self.inject_silence(n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: usize = 48; // frames per millisecond

    #[test]
    fn converts_milliseconds_to_frames() {
        assert_eq!(ms_to_frames(1), MS);
        assert_eq!(ms_to_frames(1000), 48_000);
        assert_eq!(ms_to_frames(-5), 0);
    }

    #[test]
    fn target_follows_offset_and_has_a_floor() {
        assert_eq!(target_frames(0), 60 * MS);
        assert_eq!(target_frames(300), 360 * MS);
        assert_eq!(target_frames(-30), 30 * MS);
        assert_eq!(target_frames(-300), 15 * MS);
    }

    #[test]
    fn small_deviation_is_inside_the_dead_zone() {
        let t = target_frames(0);
        assert_eq!(plan_correction(t + 14 * MS, t), Correction::None);
        assert_eq!(plan_correction(t - 14 * MS, t), Correction::None);
    }

    #[test]
    fn moderate_deviation_is_nudged_by_one_millisecond() {
        let t = target_frames(0);
        assert_eq!(plan_correction(t + 20 * MS, t), Correction::Discard(MS));
        assert_eq!(plan_correction(t - 20 * MS, t), Correction::Inject(MS));
    }

    #[test]
    fn large_deviation_snaps_to_target() {
        let t = target_frames(0);
        assert_eq!(
            plan_correction(t + 80 * MS, t),
            Correction::Discard(80 * MS)
        );
        assert_eq!(plan_correction(t - 55 * MS, t), Correction::Inject(55 * MS));
    }

    #[test]
    fn ring_is_a_fifo_with_gain() {
        let mut ring = AudioRing::new();
        ring.push(&[1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
        assert_eq!(ring.frames(), 3);

        let mut out = [0.0; 4];
        assert_eq!(ring.pop_into(&mut out, 0.5), 2);
        assert_eq!(out, [0.5, 0.5, 1.0, 1.0]);
        assert_eq!(ring.frames(), 1);
    }

    #[test]
    fn pop_reports_fewer_frames_when_underrun() {
        let mut ring = AudioRing::new();
        ring.push(&[1.0, 1.0]);
        let mut out = [9.0; 6];
        assert_eq!(ring.pop_into(&mut out, 1.0), 1);
        assert_eq!(out[2..], [9.0; 4]); // untouched beyond what was written
    }

    #[test]
    fn push_drops_oldest_frames_beyond_capacity() {
        let mut ring = AudioRing::new();
        let cap_frames = ms_to_frames(MAX_BUFFER_MS);
        let input: Vec<f32> = (0..cap_frames + 10)
            .flat_map(|i| [i as f32; CHANNELS])
            .collect();
        ring.push(&input);
        assert_eq!(ring.frames(), cap_frames);

        // The 10 oldest frames were dropped: the first one left is frame 10.
        let mut first = [0.0; CHANNELS];
        ring.pop_into(&mut first, 1.0);
        assert_eq!(first, [10.0; CHANNELS]);
    }

    #[test]
    fn discard_and_inject_change_the_fill() {
        let mut ring = AudioRing::new();
        ring.inject_silence(100);
        assert_eq!(ring.frames(), 100);
        ring.discard(30);
        assert_eq!(ring.frames(), 70);
        ring.discard(1000);
        assert_eq!(ring.frames(), 0);
    }

    #[test]
    fn applying_a_correction_adjusts_the_ring() {
        let mut ring = AudioRing::new();
        ring.inject_silence(200);
        ring.apply(Correction::Discard(50));
        assert_eq!(ring.frames(), 150);
        ring.apply(Correction::Inject(10));
        assert_eq!(ring.frames(), 160);
        ring.apply(Correction::None);
        assert_eq!(ring.frames(), 160);
    }
}
