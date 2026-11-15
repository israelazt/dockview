//! What going live needs to say about encoding and about the health of the
//! stream: the encoder ports, the compressed audio type and the stream stats.
//!
//! The encoders are implemented by the platform (Media Foundation on Windows,
//! VAAPI/NVENC/x264 on Linux); the streaming service only sees these traits.

use crate::encoded::EncodedFrame;
use crate::frame::Frame;

/// Longest gap between two keyframes that Twitch, YouTube and Kick accept.
pub const KEYFRAME_INTERVAL_SECS: u32 = 2;

/// How the video encoder is asked to work for a live stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoEncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Target bitrate in bits per second (constant bitrate).
    pub bitrate_bps: u32,
    /// A keyframe at least every this many seconds.
    pub keyframe_interval_secs: u32,
}

impl VideoEncoderConfig {
    /// A live-stream configuration: constant bitrate, a keyframe every 2 s.
    pub fn live(width: u32, height: u32, fps: u32, bitrate_kbps: u32) -> Self {
        Self {
            width,
            height,
            fps,
            bitrate_bps: bitrate_kbps.saturating_mul(1000),
            keyframe_interval_secs: KEYFRAME_INTERVAL_SECS,
        }
    }

    /// Pictures between two keyframes.
    pub fn gop_frames(&self) -> u32 {
        self.fps.saturating_mul(self.keyframe_interval_secs).max(1)
    }

    /// Whether the numbers can be used at all.
    pub fn is_valid(&self) -> bool {
        self.width >= 2
            && self.height >= 2
            && self.width.is_multiple_of(2)
            && self.height.is_multiple_of(2)
            && (1..=240).contains(&self.fps)
            && self.bitrate_bps > 0
            && self.keyframe_interval_secs > 0
    }
}

/// Turns pictures into H.264, for a live stream.
pub trait VideoEncoder: Send {
    /// Encodes one picture; the result may be empty while the encoder fills
    /// its pipeline, and may hold more than one unit when it drains.
    fn encode(&mut self, frame: &Frame) -> Result<Vec<EncodedFrame>, String>;
    /// Changes the target bitrate while running (adaptive bitrate).
    fn set_bitrate(&mut self, bitrate_bps: u32) -> Result<(), String>;
    /// Asks for a keyframe on the next picture (a new viewer, a reconnection).
    fn request_keyframe(&mut self);
}

/// One compressed AAC access unit (raw, without an ADTS header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedAudio {
    pub data: Vec<u8>,
    /// Presentation time in 100 ns units.
    pub timestamp: i64,
    /// Duration in 100 ns units.
    pub duration: i64,
}

/// What a decoder needs to read the AAC stream (the AudioSpecificConfig).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u8,
    pub specific_config: Vec<u8>,
}

/// Turns 48 kHz stereo PCM into AAC.
pub trait AudioEncoder: Send {
    /// What the player needs to decode what `encode` returns.
    fn config(&self) -> AudioConfig;
    /// Encodes interleaved 32-bit float samples stamped `timestamp` (100 ns).
    fn encode(&mut self, samples: &[f32], timestamp: i64) -> Result<Vec<EncodedAudio>, String>;
}

/// Where a live stream stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamState {
    #[default]
    Offline,
    Connecting,
    Live,
    /// The connection dropped and the service is trying again.
    Reconnecting,
    /// It gave up (bad key, refused, or too many attempts).
    Failed,
}

impl StreamState {
    /// Whether the user asked for the stream and has not stopped it.
    pub fn is_active(self) -> bool {
        !matches!(self, Self::Offline | Self::Failed)
    }
}

/// The health of a stream, shown in the interface and used by the adaptive
/// bitrate.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StreamStats {
    pub state: StreamState,
    /// Bits per second actually sent in the last second.
    pub bitrate_bps: u32,
    /// Pictures thrown away because the network could not keep up.
    pub dropped_frames: u64,
    /// Pictures sent since the stream started.
    pub sent_frames: u64,
    /// How many times it has reconnected.
    pub reconnects: u32,
}

impl StreamStats {
    /// Share of pictures dropped, from 0 to 1.
    pub fn drop_ratio(&self) -> f32 {
        let total = self.sent_frames + self.dropped_frames;
        if total == 0 {
            0.0
        } else {
            self.dropped_frames as f32 / total as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_config_is_cbr_with_a_two_second_gop() {
        let c = VideoEncoderConfig::live(1920, 1080, 60, 6000);
        assert_eq!(c.bitrate_bps, 6_000_000);
        assert_eq!(c.gop_frames(), 120);
        assert!(c.is_valid());
    }

    #[test]
    fn odd_sizes_and_zero_rates_are_invalid() {
        assert!(!VideoEncoderConfig::live(1921, 1080, 60, 6000).is_valid());
        assert!(!VideoEncoderConfig::live(1920, 1080, 0, 6000).is_valid());
        assert!(!VideoEncoderConfig::live(1920, 1080, 60, 0).is_valid());
    }

    #[test]
    fn drop_ratio_handles_no_traffic() {
        assert_eq!(StreamStats::default().drop_ratio(), 0.0);
        let s = StreamStats {
            sent_frames: 90,
            dropped_frames: 10,
            ..Default::default()
        };
        assert!((s.drop_ratio() - 0.1).abs() < 1e-6);
    }

    #[test]
    fn only_running_states_are_active() {
        assert!(StreamState::Live.is_active());
        assert!(StreamState::Reconnecting.is_active());
        assert!(!StreamState::Offline.is_active());
        assert!(!StreamState::Failed.is_active());
    }
}
