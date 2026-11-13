//! H.264 encoding to MP4 through Media Foundation.
//!
//! The sink writer is created with hardware transforms enabled, so on a PC
//! with NVENC, Quick Sync or AMD VCN the GPU's dedicated encoder does the
//! work; otherwise Windows falls back to its (much slower) software encoder.

mod encoder;
mod remuxer;
mod writer;

pub use encoder::H264Encoder;
pub use remuxer::Mp4Remuxer;
use writer::pack;
pub use writer::Mp4Writer;

use std::path::Path;

use windows::core::{Error, Interface, Result, HSTRING};
use windows::Win32::Foundation::{E_FAIL, VARIANT_BOOL};
use windows::Win32::Media::MediaFoundation::{
    eAVEncH264VProfile_High, CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode,
    CODECAPI_AVEncMPVDefaultBPictureCount, CODECAPI_AVEncMPVGOPSize, CODECAPI_AVLowLatencyMode,
    ICodecAPI, IMFActivate, IMFAttributes, IMFMediaEventGenerator, IMFMediaType, IMFSinkWriter,
    IMFTransform, METransformDrainComplete, METransformHaveOutput, METransformNeedInput,
    MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType,
    MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL, MFMediaType_Audio,
    MFMediaType_Video, MFSampleExtension_CleanPoint, MFTEnumEx, MFT_FRIENDLY_NAME_Attribute,
    MFTranscodeContainerType_FMPEG4, MFVideoFormat_H264, MFVideoFormat_NV12,
    MFVideoInterlace_Progressive, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG_HARDWARE,
    MFT_ENUM_FLAG_SORTANDFILTER, MFT_MESSAGE_COMMAND_DRAIN, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_END_STREAMING, MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFT_REGISTER_TYPE_INFO, MF_EVENT_FLAG_NO_WAIT,
    MF_E_NO_EVENTS_AVAILABLE, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_LOW_LATENCY,
    MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, MF_MT_AAC_PAYLOAD_TYPE,
    MF_MT_ALL_SAMPLES_INDEPENDENT, MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE,
    MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
    MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_MPEG_SEQUENCE_HEADER,
    MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
    MF_SINK_WRITER_DISABLE_THROTTLING, MF_TRANSCODE_CONTAINERTYPE, MF_TRANSFORM_ASYNC_UNLOCK,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Variant::{
    VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_UI4,
};

use dockview_domain::encoded::EncodedFrame;

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

/// A `VT_UI4` variant, as the codec API expects for numeric settings.
fn variant_u32(value: u32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_UI4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { ulVal: value },
            }),
        },
    }
}

/// A `VT_BOOL` variant (`VARIANT_TRUE` is -1).
fn variant_bool(value: bool) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_BOOL,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 {
                    boolVal: VARIANT_BOOL(if value { -1 } else { 0 }),
                },
            }),
        },
    }
}

/// Reads the SPS/PPS blob out of an encoder output type, when present.
pub fn sequence_header(media_type: &IMFMediaType) -> Option<Vec<u8>> {
    // SAFETY: GetBlobSize/GetBlob follow the documented two-step protocol.
    unsafe {
        let size = media_type.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER).ok()?;
        let mut blob = vec![0u8; size as usize];
        media_type
            .GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None)
            .ok()?;
        Some(blob)
    }
}
