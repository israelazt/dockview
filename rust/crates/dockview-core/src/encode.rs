//! H.264 encoding to MP4 through Media Foundation.
//!
//! The sink writer is created with hardware transforms enabled, so on a PC
//! with NVENC, Quick Sync or AMD VCN the GPU's dedicated encoder does the
//! work; otherwise Windows falls back to its (much slower) software encoder.

use std::path::Path;

use windows::core::{Error, Result, HSTRING};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Media::MediaFoundation::{
    eAVEncH264VProfile_High, IMFActivate, IMFAttributes, IMFSinkWriter, MFCreateAttributes,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL,
    MFMediaType_Video, MFTEnumEx, MFT_FRIENDLY_NAME_Attribute, MFVideoFormat_H264,
    MFVideoFormat_NV12, MFVideoInterlace_Progressive, MFT_CATEGORY_VIDEO_ENCODER,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_REGISTER_TYPE_INFO, MF_MT_AVG_BITRATE,
    MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
    MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
    MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_SINK_WRITER_DISABLE_THROTTLING,
};
use windows::Win32::System::Com::CoTaskMemFree;

use crate::mf::{get_string, MfSession};

/// Length of one video frame in 100 ns units at `fps`.
pub fn frame_duration_100ns(fps: u32) -> i64 {
    10_000_000 / i64::from(fps.max(1))
}

/// Bytes in a tightly packed NV12 frame.
pub fn nv12_len(width: u32, height: u32) -> usize {
    width as usize * height as usize * 3 / 2
}

/// Names of the hardware H.264 encoders Windows can use on this PC, best
/// first. Empty when only the software encoder exists.
pub fn hardware_h264_encoders(_session: &MfSession) -> Result<Vec<String>> {
    let output = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut array: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: standard MFTEnumEx call; the returned CoTaskMem array is freed
    // below after taking ownership of each element.
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            Some(&output),
            &mut array,
            &mut count,
        )?;
        if array.is_null() {
            return Ok(Vec::new());
        }
        let slots = std::slice::from_raw_parts_mut(array, count as usize);
        let names = slots
            .iter_mut()
            .filter_map(Option::take)
            .map(|activate| {
                get_string(&activate, &MFT_FRIENDLY_NAME_Attribute)
                    .unwrap_or_else(|| "(unnamed encoder)".into())
            })
            .collect();
        CoTaskMemFree(Some(array as *const _));
        Ok(names)
    }
}

/// What to encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodeSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_bps: u32,
}

/// Writes NV12 frames to an H.264 MP4 file.
pub struct Mp4Writer {
    writer: IMFSinkWriter,
    stream: u32,
    frame_len: usize,
    frame_duration: i64,
}

fn pack(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

impl Mp4Writer {
    /// Creates `path` and prepares the encoder.
    pub fn create(_session: &MfSession, path: &Path, settings: &EncodeSettings) -> Result<Self> {
        let EncodeSettings {
            width,
            height,
            fps,
            bitrate_bps,
        } = *settings;
        // SAFETY: media types and attributes are plain COM objects created and
        // configured with documented attribute keys before being handed over.
        unsafe {
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 2)?;
            let attrs = attrs.ok_or_else(|| Error::new(E_FAIL, "no attribute store"))?;
            attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            attrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;

            let url = HSTRING::from(path.as_os_str());
            let writer = MFCreateSinkWriterFromURL(&url, None, &attrs)?;

            // What the file will contain.
            let out = MFCreateMediaType()?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate_bps)?;
            out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
            out.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
            out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
            let stream = writer.AddStream(&out)?;

            // What we feed it.
            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
            input.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
            input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            input.SetUINT32(&MF_MT_DEFAULT_STRIDE, width)?;
            writer.SetInputMediaType(stream, &input, None::<&IMFAttributes>)?;
            writer.BeginWriting()?;

            Ok(Self {
                writer,
                stream,
                frame_len: nv12_len(width, height),
                frame_duration: frame_duration_100ns(fps),
            })
        }
    }

    /// Encodes one tightly packed NV12 frame shown at `timestamp_100ns`.
    pub fn write_nv12(&mut self, nv12: &[u8], timestamp_100ns: i64) -> Result<()> {
        if nv12.len() < self.frame_len {
            return Err(Error::new(
                E_FAIL,
                "NV12 frame is smaller than the configured size",
            ));
        }
        // SAFETY: the buffer is locked only while copying `frame_len` bytes into
        // memory that was allocated with at least that capacity.
        unsafe {
            let buffer = MFCreateMemoryBuffer(self.frame_len as u32)?;
            let mut data: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            std::ptr::copy_nonoverlapping(nv12.as_ptr(), data, self.frame_len);
            buffer.Unlock()?;
            buffer.SetCurrentLength(self.frame_len as u32)?;

            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(timestamp_100ns)?;
            sample.SetSampleDuration(self.frame_duration)?;
            self.writer.WriteSample(self.stream, &sample)
        }
    }

    /// Flushes the encoder and finalises the MP4. Must be called for the file
    /// to be playable.
    pub fn finish(self) -> Result<()> {
        // SAFETY: the writer is valid and was started with BeginWriting.
        unsafe { self.writer.Finalize() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_durations_match_common_rates() {
        assert_eq!(frame_duration_100ns(60), 166_666);
        assert_eq!(frame_duration_100ns(30), 333_333);
        assert_eq!(
            frame_duration_100ns(0),
            10_000_000,
            "a zero rate is treated as 1 fps"
        );
    }

    #[test]
    fn nv12_frame_size_is_one_and_a_half_bytes_per_pixel() {
        assert_eq!(nv12_len(1920, 1080), 3_110_400);
        assert_eq!(nv12_len(1080, 1920), 3_110_400);
    }

    #[test]
    fn a_short_clip_encodes_to_a_playable_file() {
        // Uses whatever encoder this machine has (hardware or software).
        let session = MfSession::start().expect("media foundation");
        let path = std::env::temp_dir().join(format!("dockview-enc-{}.mp4", std::process::id()));
        let settings = EncodeSettings {
            width: 640,
            height: 360,
            fps: 30,
            bitrate_bps: 2_000_000,
        };
        let mut writer = Mp4Writer::create(&session, &path, &settings).expect("create writer");
        let frame = crate::testsrc::nv12_bars(640, 360);
        for i in 0..30 {
            writer
                .write_nv12(&frame.data, i * frame_duration_100ns(30))
                .expect("write frame");
        }
        writer.finish().expect("finalize");
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let _ = std::fs::remove_file(&path);
        assert!(size > 1_000, "encoded file is {size} bytes");
    }

    #[test]
    fn a_frame_that_is_too_small_is_rejected() {
        let session = MfSession::start().expect("media foundation");
        let path =
            std::env::temp_dir().join(format!("dockview-enc-small-{}.mp4", std::process::id()));
        let settings = EncodeSettings {
            width: 320,
            height: 180,
            fps: 30,
            bitrate_bps: 1_000_000,
        };
        let mut writer = Mp4Writer::create(&session, &path, &settings).expect("create writer");
        assert!(writer.write_nv12(&[0u8; 10], 0).is_err());
        drop(writer);
        let _ = std::fs::remove_file(&path);
    }
}
