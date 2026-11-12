//! Audio for the replay buffer: a timeline of 16-bit stereo PCM aligned to the
//! video clock, and the queue that carries samples from the capture thread.
//!
//! Video frames are stamped with the instant they arrive; audio arrives in
//! small chunks whose arrival is stamped the same way. The audio device and
//! the capture card run on different clocks, so counting samples alone would
//! slowly drift away from the picture. [`PcmRing`] therefore follows the wall
//! clock: it fills small gaps with silence and drops overlaps, as the live
//! playback path does.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::audio_sync::{CHANNELS, SAMPLE_RATE};

/// 100 ns units per second.
const TICKS_PER_SECOND: i64 = 10_000_000;
/// Gaps or overlaps smaller than this are timing jitter and are ignored.
pub const DRIFT_LIMIT_TICKS: i64 = 200_000; // 20 ms
/// A gap bigger than this means the device stopped; the timeline restarts.
const RESTART_GAP_TICKS: i64 = 5 * TICKS_PER_SECOND;
/// Extra footage kept beyond the requested window, so trimming never cuts
/// audio that a clip still needs.
const KEEP_MARGIN_TICKS: i64 = 3 * TICKS_PER_SECOND;

/// Audio frames (one sample per channel) that last `ticks`.
pub fn ticks_to_frames(ticks: i64) -> i64 {
    ticks * i64::from(SAMPLE_RATE) / TICKS_PER_SECOND
}

/// Duration in 100 ns units of `frames` audio frames.
pub fn frames_to_ticks(frames: i64) -> i64 {
    frames * TICKS_PER_SECOND / i64::from(SAMPLE_RATE)
}

fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

/// Interleaved 16-bit stereo PCM on a single timeline.
#[derive(Debug)]
pub struct PcmRing {
    samples: VecDeque<i16>,
    /// Timestamp of the first held frame.
    front_ts: i64,
    keep: i64,
}

impl PcmRing {
    pub fn new(keep_seconds: f64) -> Self {
        Self {
            samples: VecDeque::new(),
            front_ts: 0,
            keep: (keep_seconds.max(0.0) * TICKS_PER_SECOND as f64) as i64,
        }
    }

    pub fn set_keep_seconds(&mut self, seconds: f64) {
        self.keep = (seconds.max(0.0) * TICKS_PER_SECOND as f64) as i64;
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Frames (not samples) currently held.
    pub fn frames(&self) -> usize {
        self.samples.len() / CHANNELS
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(SAMPLE_RATE)
    }

    /// Timestamp just after the last held frame.
    pub fn end_ts(&self) -> Option<i64> {
        (!self.is_empty()).then(|| self.front_ts + frames_to_ticks(self.frames() as i64))
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Appends `interleaved` stereo samples that began at `start_ts`.
    ///
    /// Small timing differences are ignored; larger gaps are filled with
    /// silence and overlaps are skipped so the audio stays locked to the
    /// clock that stamped the video.
    pub fn push(&mut self, start_ts: i64, interleaved: &[f32]) {
        let mut data = interleaved;
        match self.end_ts() {
            None => self.front_ts = start_ts,
            Some(end) => {
                let drift = start_ts - end;
                if drift > RESTART_GAP_TICKS || -drift > RESTART_GAP_TICKS {
                    self.samples.clear();
                    self.front_ts = start_ts;
                } else if drift > DRIFT_LIMIT_TICKS {
                    let gap = ticks_to_frames(drift) as usize * CHANNELS;
                    self.samples.extend(std::iter::repeat_n(0i16, gap));
                } else if drift < -DRIFT_LIMIT_TICKS {
                    let skip = (ticks_to_frames(-drift) as usize * CHANNELS).min(data.len());
                    data = &data[skip..];
                }
            }
        }
        self.samples.extend(data.iter().map(|&s| to_i16(s)));
        self.trim();
    }

    /// Drops audio older than the window (plus a margin), whole frames only.
    fn trim(&mut self) {
        let Some(end) = self.end_ts() else {
            return;
        };
        let oldest_wanted = end - self.keep - KEEP_MARGIN_TICKS;
        if oldest_wanted <= self.front_ts {
            return;
        }
        let drop_frames =
            (ticks_to_frames(oldest_wanted - self.front_ts) as usize).min(self.frames());
        self.samples.drain(..drop_frames * CHANNELS);
        self.front_ts += frames_to_ticks(drop_frames as i64);
    }

    /// The audio between `from_ts` and `to_ts`, silence-padded where the
    /// timeline does not cover it, as interleaved stereo.
    pub fn extract(&self, from_ts: i64, to_ts: i64) -> Vec<i16> {
        let total = ticks_to_frames((to_ts - from_ts).max(0)) as usize;
        let mut out = vec![0i16; total * CHANNELS];
        let offset = ticks_to_frames(from_ts - self.front_ts); // may be negative
        for frame in 0..total as i64 {
            let source = offset + frame;
            if source < 0 || source as usize >= self.frames() {
                continue;
            }
            let (s, d) = (source as usize * CHANNELS, frame as usize * CHANNELS);
            for c in 0..CHANNELS {
                out[d + c] = self.samples[s + c];
            }
        }
        out
    }
}

/// A block of captured audio waiting for the recorder.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// When the first sample was captured.
    pub start: Instant,
    pub samples: Vec<f32>,
}

/// Bounded queue from the audio capture thread to the recorder. Like the
/// frame tap, a slow consumer drops new chunks instead of blocking capture.
pub struct AudioTap {
    queue: Mutex<VecDeque<AudioChunk>>,
    ready: Condvar,
    capacity: usize,
    dropped: AtomicU64,
}

impl AudioTap {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            ready: Condvar::new(),
            capacity: capacity.max(1),
            dropped: AtomicU64::new(0),
        }
    }

    /// Queues `samples` that were *received* at `arrived`; the chunk's start
    /// is that instant minus its own length.
    pub fn offer(&self, arrived: Instant, samples: &[f32]) {
        let length =
            Duration::from_secs_f64((samples.len() / CHANNELS) as f64 / f64::from(SAMPLE_RATE));
        let start = arrived.checked_sub(length).unwrap_or(arrived);
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        if queue.len() >= self.capacity {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        queue.push_back(AudioChunk {
            start,
            samples: samples.to_vec(),
        });
        drop(queue);
        self.ready.notify_one();
    }

    /// The oldest chunk, waiting up to `timeout`.
    pub fn take(&self, timeout: Duration) -> Option<AudioChunk> {
        let queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        let (mut queue, _) = self
            .ready
            .wait_timeout_while(queue, timeout, |q| q.is_empty())
            .unwrap_or_else(|e| e.into_inner());
        queue.pop_front()
    }

    /// Chunks dropped because the consumer was too slow.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `frames` of a constant stereo value.
    fn tone(frames: usize, value: f32) -> Vec<f32> {
        vec![value; frames * CHANNELS]
    }

    fn second() -> i64 {
        TICKS_PER_SECOND
    }

    #[test]
    fn frame_and_tick_conversions_round_trip_at_whole_seconds() {
        assert_eq!(ticks_to_frames(second()), 48_000);
        assert_eq!(frames_to_ticks(48_000), second());
        assert_eq!(ticks_to_frames(0), 0);
    }

    #[test]
    fn samples_convert_to_16_bit_with_clipping() {
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(1.0), i16::MAX);
        assert_eq!(to_i16(-1.0), -i16::MAX);
        assert_eq!(to_i16(5.0), i16::MAX, "clipped, not wrapped");
        assert_eq!(to_i16(-5.0), -i16::MAX);
    }

    #[test]
    fn contiguous_chunks_form_one_continuous_timeline() {
        let mut ring = PcmRing::new(10.0);
        ring.push(second(), &tone(4_800, 0.5)); // 100 ms
        ring.push(second() + frames_to_ticks(4_800), &tone(4_800, 0.5));
        assert_eq!(ring.frames(), 9_600);
        assert_eq!(ring.end_ts(), Some(second() + frames_to_ticks(9_600)));
    }

    #[test]
    fn a_gap_is_filled_with_silence_so_the_audio_stays_on_the_clock() {
        let mut ring = PcmRing::new(10.0);
        ring.push(0, &tone(4_800, 0.5));
        // The next chunk starts 200 ms late: 100 ms of audio, then 200 ms gap.
        ring.push(frames_to_ticks(4_800) + 2_000_000, &tone(4_800, 0.5));
        assert_eq!(ring.frames(), 4_800 + 9_600 + 4_800);
        let gap = ring.extract(frames_to_ticks(4_800), frames_to_ticks(4_800) + 2_000_000);
        assert!(gap.iter().all(|&s| s == 0), "the gap is silent");
    }

    #[test]
    fn small_jitter_is_ignored_and_overlap_is_skipped() {
        let mut ring = PcmRing::new(10.0);
        ring.push(0, &tone(4_800, 0.5));
        // 10 ms late: inside the 20 ms tolerance, so no silence is inserted.
        ring.push(frames_to_ticks(4_800) + 100_000, &tone(4_800, 0.5));
        assert_eq!(ring.frames(), 9_600);
        // A chunk that starts 100 ms too early: its first 100 ms is a repeat.
        let mut ring = PcmRing::new(10.0);
        ring.push(0, &tone(9_600, 0.5));
        ring.push(frames_to_ticks(4_800), &tone(9_600, 0.5));
        assert_eq!(ring.frames(), 9_600 + 4_800);
    }

    #[test]
    fn a_long_silence_restarts_the_timeline() {
        let mut ring = PcmRing::new(10.0);
        ring.push(0, &tone(4_800, 0.5));
        ring.push(60 * second(), &tone(4_800, 0.5));
        assert_eq!(ring.frames(), 4_800, "no minute of silence was stored");
        assert_eq!(ring.end_ts(), Some(60 * second() + frames_to_ticks(4_800)));
    }

    #[test]
    fn old_audio_is_trimmed_but_the_window_is_kept() {
        let mut ring = PcmRing::new(2.0);
        let mut ts = 0;
        for _ in 0..100 {
            ring.push(ts, &tone(4_800, 0.25)); // 10 s in 100 ms chunks
            ts += frames_to_ticks(4_800);
        }
        assert!(ring.duration_seconds() >= 2.0);
        assert!(
            ring.duration_seconds() < 2.0 + 3.0 + 0.2,
            "{}",
            ring.duration_seconds()
        );
        // The newest second is still there.
        let last = ring.extract(ts - second(), ts);
        assert!(last.iter().all(|&s| s != 0));
    }

    #[test]
    fn extract_returns_exactly_the_requested_length_and_pads_with_silence() {
        let mut ring = PcmRing::new(10.0);
        ring.push(second(), &tone(48_000, 0.5)); // covers 1 s .. 2 s
        let out = ring.extract(0, 3 * second());
        assert_eq!(out.len(), 3 * 48_000 * CHANNELS);
        let (before, during, after) = (
            &out[..48_000 * 2],
            &out[48_000 * 2..48_000 * 4],
            &out[48_000 * 4..],
        );
        assert!(
            before.iter().all(|&s| s == 0),
            "silence before the audio began"
        );
        assert!(during.iter().all(|&s| s != 0));
        assert!(after.iter().all(|&s| s == 0), "silence after it ended");
    }

    #[test]
    fn extracting_an_empty_range_is_empty() {
        let ring = PcmRing::new(5.0);
        assert!(ring.extract(10, 10).is_empty());
        assert!(ring.extract(10, 5).is_empty());
    }

    #[test]
    fn the_tap_stamps_chunks_with_their_start_not_their_arrival() {
        let tap = AudioTap::new(4);
        let arrived = Instant::now();
        tap.offer(arrived, &tone(4_800, 0.1)); // 100 ms long
        let chunk = tap.take(Duration::from_millis(5)).unwrap();
        let back = arrived.duration_since(chunk.start).as_millis();
        assert!(
            (98..=102).contains(&back),
            "started {back} ms before arrival"
        );
        assert_eq!(chunk.samples.len(), 4_800 * CHANNELS);
    }

    #[test]
    fn a_full_tap_drops_and_counts() {
        let tap = AudioTap::new(2);
        for _ in 0..5 {
            tap.offer(Instant::now(), &tone(480, 0.1));
        }
        assert_eq!(tap.dropped(), 3);
        assert!(tap.take(Duration::from_millis(1)).is_some());
        assert!(tap.take(Duration::from_millis(1)).is_some());
        assert!(tap.take(Duration::from_millis(1)).is_none());
    }
}
