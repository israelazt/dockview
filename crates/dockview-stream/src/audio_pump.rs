//! Turns the game's sound and the microphone, which arrive as chunks stamped
//! with the moment they were captured, into the steady blocks the AAC encoder
//! wants, mixed and lined up on the same clock as the video.

use dockview_domain::audio_sync::{CHANNELS, SAMPLE_RATE};
use dockview_domain::broadcast::{AudioEncoder, EncodedAudio};
use dockview_domain::pcm::{frames_to_ticks, mix_tracks, PcmRing};

/// Audio frames in one block (one AAC unit).
pub const BLOCK_FRAMES: i64 = 1024;
/// How far behind "now" audio is cut, so slow sources have time to arrive.
const DELAY_TICKS: i64 = 1_000_000; // 100 ms
/// More audio than this behind "now" is not worth catching up on.
const MAX_BEHIND_TICKS: i64 = 20_000_000; // 2 s
/// Seconds of each source kept.
const KEEP_SECONDS: f64 = 4.0;

pub struct AudioPump {
    game: PcmRing,
    mic: PcmRing,
    /// Where the timeline began, on the clock the video uses.
    start: i64,
    /// Blocks produced so far; counting them (rather than adding rounded
    /// durations) keeps the timeline exact.
    blocks: i64,
}

impl AudioPump {
    /// `start` is where the audio timeline begins (100 ns).
    pub fn new(start: i64) -> Self {
        Self {
            game: PcmRing::new(KEEP_SECONDS),
            mic: PcmRing::new(KEEP_SECONDS),
            start,
            blocks: 0,
        }
    }

    /// Adds game sound that began at `start_ts`.
    pub fn push_game(&mut self, start_ts: i64, samples: &[f32]) {
        self.game.push(start_ts, samples);
    }

    /// Adds microphone sound that began at `start_ts`.
    pub fn push_mic(&mut self, start_ts: i64, samples: &[f32]) {
        self.mic.push(start_ts, samples);
    }

    /// Forgets the microphone (it was turned off).
    pub fn clear_mic(&mut self) {
        self.mic.clear();
    }

    /// Start of the next block.
    fn cursor(&self) -> i64 {
        self.start + frames_to_ticks(self.blocks * BLOCK_FRAMES)
    }

    /// Encodes every block that is due at `now` (100 ns on the same clock).
    /// A source with nothing to say counts as silence, so the stream keeps its
    /// audio even with no sound at all.
    pub fn pump(
        &mut self,
        now: i64,
        encoder: &mut dyn AudioEncoder,
    ) -> Result<Vec<EncodedAudio>, String> {
        let block = frames_to_ticks(BLOCK_FRAMES);
        let due = now - DELAY_TICKS;
        if due - self.cursor() > MAX_BEHIND_TICKS {
            // Stalled for a long time: jump forward instead of flooding.
            let skip = (due - self.cursor() - block) / block;
            self.blocks += skip;
        }
        let mut out = Vec::new();
        while self.cursor() + block <= due {
            let from = self.cursor();
            // A little extra, because a block's length in ticks is rounded down
            // to a whole number of audio frames; the extra is cut off.
            let wanted = BLOCK_FRAMES as usize * CHANNELS;
            let mut game = self.game.extract(from, from + block + 300);
            let mut mic = self.mic.extract(from, from + block + 300);
            game.truncate(wanted);
            mic.truncate(wanted);
            let samples: Vec<f32> = mix_tracks(&game, &mic)
                .iter()
                .map(|&s| f32::from(s) / 32768.0)
                .collect();
            out.extend(encoder.encode(&samples, from)?);
            self.blocks += 1;
        }
        Ok(out)
    }
}

/// The sample rate the pump works at.
pub const fn sample_rate() -> u32 {
    SAMPLE_RATE
}

#[cfg(test)]
mod tests {
    use super::*;
    use dockview_domain::broadcast::AudioConfig;

    /// Records what it was asked to encode and answers one unit per block.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<(i64, Vec<f32>)>,
    }

    impl AudioEncoder for Recorder {
        fn config(&self) -> AudioConfig {
            AudioConfig {
                sample_rate: SAMPLE_RATE,
                channels: 2,
                specific_config: vec![],
            }
        }

        fn encode(&mut self, samples: &[f32], timestamp: i64) -> Result<Vec<EncodedAudio>, String> {
            self.calls.push((timestamp, samples.to_vec()));
            Ok(vec![EncodedAudio {
                data: vec![1],
                timestamp,
                duration: frames_to_ticks(BLOCK_FRAMES),
            }])
        }
    }

    fn constant(frames: usize, v: f32) -> Vec<f32> {
        vec![v; frames * CHANNELS]
    }

    #[test]
    fn nothing_is_due_before_the_delay_and_a_block() {
        let mut pump = AudioPump::new(0);
        let mut enc = Recorder::default();
        assert!(pump.pump(1_000_000 + 100_000, &mut enc).unwrap().is_empty());
        assert!(enc.calls.is_empty());
    }

    #[test]
    fn silence_is_sent_when_no_source_speaks() {
        let mut pump = AudioPump::new(0);
        let mut enc = Recorder::default();
        let units = pump.pump(5_000_000, &mut enc).unwrap(); // 0.5 s
        assert_eq!(units.len(), 18); // (0.5 - 0.1) s / 21.33 ms
        assert!(enc.calls.iter().all(|(_, s)| s.iter().all(|&x| x == 0.0)));
        assert_eq!(enc.calls[1].0, frames_to_ticks(BLOCK_FRAMES));
    }

    #[test]
    fn game_and_microphone_are_mixed_on_the_same_clock() {
        let mut pump = AudioPump::new(0);
        pump.push_game(0, &constant(48_000, 0.25));
        pump.push_mic(0, &constant(48_000, 0.125));
        let mut enc = Recorder::default();
        pump.pump(5_000_000, &mut enc).unwrap();
        let first = &enc.calls[0].1;
        assert!((first[10] - 0.375).abs() < 0.001, "{}", first[10]);
    }

    #[test]
    fn a_source_that_starts_late_leaves_silence_before_it() {
        let mut pump = AudioPump::new(0);
        let late = frames_to_ticks(BLOCK_FRAMES) * 2;
        pump.push_game(late, &constant(24_000, 0.5));
        let mut enc = Recorder::default();
        pump.pump(5_000_000, &mut enc).unwrap();
        assert_eq!(enc.calls[0].1[0], 0.0);
        assert!(enc.calls[3].1[0] > 0.4);
    }

    #[test]
    fn a_long_stall_jumps_ahead_instead_of_flooding() {
        let mut pump = AudioPump::new(0);
        let mut enc = Recorder::default();
        let units = pump.pump(600_000_000, &mut enc).unwrap(); // a minute later
        assert!(units.len() < 120, "{} blocks", units.len());
        assert!(enc.calls[0].0 > 500_000_000);
    }

    #[test]
    fn an_encoder_error_is_returned() {
        struct Broken;
        impl AudioEncoder for Broken {
            fn config(&self) -> AudioConfig {
                AudioConfig {
                    sample_rate: SAMPLE_RATE,
                    channels: 2,
                    specific_config: vec![],
                }
            }
            fn encode(&mut self, _: &[f32], _: i64) -> Result<Vec<EncodedAudio>, String> {
                Err("boom".into())
            }
        }
        let mut pump = AudioPump::new(0);
        assert_eq!(pump.pump(5_000_000, &mut Broken).unwrap_err(), "boom");
    }
}
