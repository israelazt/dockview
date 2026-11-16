//! The video encoder for a live stream: the hardware H.264 encoder with a
//! constant bitrate, a keyframe every two seconds and a bitrate that can change
//! while it runs.

use dockview_domain::broadcast::{VideoEncoder, VideoEncoderConfig};
use dockview_domain::encoded::EncodedFrame;
use dockview_domain::frame::{Frame, PixelFormat};

use super::*;

pub struct LiveVideoEncoder {
    encoder: H264Encoder,
    config: VideoEncoderConfig,
    scratch: Vec<u8>,
}

impl LiveVideoEncoder {
    /// Opens the hardware encoder for `config`. Fails when the numbers make no
    /// sense or the PC has no hardware H.264 encoder.
    pub fn new(session: &MfSession, config: VideoEncoderConfig) -> Result<Self> {
        if !config.is_valid() {
            return Err(Error::new(E_FAIL, "the stream settings are not valid"));
        }
        let settings = EncodeSettings {
            width: config.width,
            height: config.height,
            fps: config.fps,
            bitrate_bps: config.bitrate_bps,
        };
        let encoder = H264Encoder::with_gop(session, &settings, config.gop_frames())?;
        Ok(Self {
            encoder,
            config,
            scratch: Vec::new(),
        })
    }
}

impl VideoEncoder for LiveVideoEncoder {
    fn encode(&mut self, frame: &Frame) -> std::result::Result<Vec<EncodedFrame>, String> {
        if frame.format != PixelFormat::Nv12 {
            return Err("only NV12 pictures can be streamed".into());
        }
        if (frame.width, frame.height) != (self.config.width, self.config.height) {
            return Err(format!(
                "the picture is {}x{} but the stream is {}x{}",
                frame.width, frame.height, self.config.width, self.config.height
            ));
        }
        let data = tight_nv12(frame, &mut self.scratch);
        self.encoder
            .encode(data, frame.timestamp)
            .map_err(|e| e.message())
    }

    fn set_bitrate(&mut self, bitrate_bps: u32) -> std::result::Result<(), String> {
        self.encoder
            .set_bitrate(bitrate_bps)
            .map_err(|e| e.message())?;
        self.config.bitrate_bps = bitrate_bps;
        Ok(())
    }

    fn request_keyframe(&mut self) {
        let _ = self.encoder.request_keyframe();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::hardware_or_skip;
    use dockview_domain::frame::ColorInfo;
    use std::time::Instant;

    fn picture(w: u32, h: u32, n: i64) -> Frame {
        let mut data = vec![128u8; nv12_len(w, h)];
        // Some movement so the encoder has something to do.
        for (i, b) in data.iter_mut().take((w * h) as usize).enumerate() {
            *b = ((i as i64 + n * 7) % 220) as u8 + 16;
        }
        Frame {
            width: w,
            height: h,
            stride: w,
            format: PixelFormat::Nv12,
            color: ColorInfo::default(),
            data,
            timestamp: n * frame_duration_100ns(30),
            arrived: Instant::now(),
        }
    }

    #[test]
    fn rejects_a_configuration_that_cannot_work() {
        let session = MfSession::start().unwrap();
        let bad = VideoEncoderConfig::live(1921, 1080, 30, 3000);
        assert!(LiveVideoEncoder::new(&session, bad).is_err());
    }

    #[test]
    fn streams_with_keyframes_and_follows_a_bitrate_change() {
        if !hardware_or_skip("a hardware H.264 encoder") {
            return;
        }
        let session = MfSession::start().unwrap();
        let config = VideoEncoderConfig::live(640, 360, 30, 2000);
        let Ok(mut enc) = LiveVideoEncoder::new(&session, config) else {
            eprintln!("no hardware H.264 encoder: skipping");
            return;
        };
        let mut out = Vec::new();
        for n in 0..70 {
            out.extend(enc.encode(&picture(640, 360, n)).unwrap());
            if n == 20 {
                enc.set_bitrate(1_000_000).unwrap();
            }
            if n == 40 {
                enc.request_keyframe();
            }
        }
        assert!(out.len() > 30, "got {} pictures", out.len());
        assert!(out[0].keyframe);
        let keys = out.iter().filter(|f| f.keyframe).count();
        assert!(keys >= 2, "a keyframe every two seconds or on request");
    }

    #[test]
    fn a_picture_of_another_size_is_an_error() {
        let session = MfSession::start().unwrap();
        let Ok(mut enc) =
            LiveVideoEncoder::new(&session, VideoEncoderConfig::live(640, 360, 30, 2000))
        else {
            return;
        };
        assert!(enc.encode(&picture(320, 180, 0)).is_err());
    }
}
