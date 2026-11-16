//! The payloads of FLV tags, which are also the payloads of RTMP video and
//! audio messages. (RTMP carries them without the FLV file framing.)

use dockview_domain::broadcast::{AudioConfig, EncodedAudio};
use dockview_domain::encoded::EncodedFrame;

use crate::annexb;

/// 100 ns units in a millisecond.
const TICKS_PER_MS: i64 = 10_000;

/// A timestamp in milliseconds, as FLV and RTMP count (32 bits, wrapping).
pub fn millis(ticks: i64) -> u32 {
    (ticks.max(0) / TICKS_PER_MS) as u32
}

/// The video payload that tells the player how to read the stream: the
/// AVCDecoderConfigurationRecord built from the SPS and PPS.
pub fn video_sequence_header(sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut b = vec![0x17, 0, 0, 0, 0]; // keyframe + AVC, sequence header, cts 0
    b.extend([1, sps[1], sps[2], sps[3], 0xFF, 0xE1]);
    b.extend((sps.len() as u16).to_be_bytes());
    b.extend(sps);
    b.push(1);
    b.extend((pps.len() as u16).to_be_bytes());
    b.extend(pps);
    b
}

/// The video payload of one picture (no B-frames, so the composition offset
/// is 0). `None` when the picture has nothing to show after the parameter
/// sets are taken out.
pub fn video_payload(frame: &EncodedFrame) -> Option<Vec<u8>> {
    let avcc = annexb::to_avcc(&frame.data);
    if avcc.is_empty() {
        return None;
    }
    let kind = if frame.keyframe { 0x17 } else { 0x27 };
    let mut b = vec![kind, 1, 0, 0, 0]; // AVC NALU, cts 0
    b.extend(avcc);
    Some(b)
}

/// The audio payload that carries the AudioSpecificConfig.
pub fn audio_sequence_header(config: &AudioConfig) -> Vec<u8> {
    let mut b = vec![audio_flags(config), 0];
    b.extend(&config.specific_config);
    b
}

/// The audio payload of one AAC unit.
pub fn audio_payload(config: &AudioConfig, unit: &EncodedAudio) -> Vec<u8> {
    let mut b = vec![audio_flags(config), 1];
    b.extend(&unit.data);
    b
}

/// AAC (10) in the high nibble; the rate, size and channel bits are fixed by
/// the format at 44.1 kHz, 16 bit, stereo whatever the real values are.
fn audio_flags(config: &AudioConfig) -> u8 {
    0xA0 | 0x0C | 0x02 | u8::from(config.channels > 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(keyframe: bool) -> EncodedFrame {
        EncodedFrame {
            data: vec![0, 0, 0, 1, 0x65, 9, 9],
            timestamp: 0,
            duration: 0,
            keyframe,
        }
    }

    #[test]
    fn converts_ticks_to_milliseconds() {
        assert_eq!(millis(10_000), 1);
        assert_eq!(millis(25_000_000), 2500);
        assert_eq!(millis(-5), 0);
    }

    #[test]
    fn sequence_header_holds_both_parameter_sets() {
        let h = video_sequence_header(&[0x67, 0x64, 0x00, 0x28], &[0x68, 0xEE]);
        assert_eq!(&h[..5], [0x17, 0, 0, 0, 0]);
        assert_eq!(&h[5..11], [1, 0x64, 0x00, 0x28, 0xFF, 0xE1]);
        assert_eq!(&h[11..13], [0, 4]);
        assert_eq!(&h[13..17], [0x67, 0x64, 0x00, 0x28]);
        assert_eq!(&h[17..20], [1, 0, 2]);
        assert_eq!(&h[20..], [0x68, 0xEE]);
    }

    #[test]
    fn picture_payload_marks_keyframes() {
        let key = video_payload(&frame(true)).unwrap();
        let delta = video_payload(&frame(false)).unwrap();
        assert_eq!(&key[..5], [0x17, 1, 0, 0, 0]);
        assert_eq!(delta[0], 0x27);
        assert_eq!(&key[5..], [0, 0, 0, 3, 0x65, 9, 9]);
    }

    #[test]
    fn a_picture_with_only_parameter_sets_has_no_payload() {
        let f = EncodedFrame {
            data: vec![0, 0, 0, 1, 0x67, 1, 2, 3],
            ..frame(true)
        };
        assert!(video_payload(&f).is_none());
    }

    #[test]
    fn audio_payloads_are_aac_stereo() {
        let cfg = AudioConfig {
            sample_rate: 48_000,
            channels: 2,
            specific_config: vec![0x11, 0x90],
        };
        assert_eq!(audio_sequence_header(&cfg), [0xAF, 0, 0x11, 0x90]);
        let unit = EncodedAudio {
            data: vec![1, 2, 3],
            timestamp: 0,
            duration: 0,
        };
        assert_eq!(audio_payload(&cfg, &unit), [0xAF, 1, 1, 2, 3]);
    }
}
