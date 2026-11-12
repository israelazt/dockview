//! H.264 encoding to MP4 through Media Foundation.
//!
//! The sink writer is created with hardware transforms enabled, so on a PC
//! with NVENC, Quick Sync or AMD VCN the GPU's dedicated encoder does the
//! work; otherwise Windows falls back to its (much slower) software encoder.

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

// ===========================================================================
// Direct control of the encoder: compressed samples out, no file involved.
// ===========================================================================

/// One compressed H.264 access unit (Annex B), ready to be stored or muxed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub data: Vec<u8>,
    /// Presentation time in 100 ns units.
    pub timestamp: i64,
    /// Duration in 100 ns units.
    pub duration: i64,
    /// A clean point: decoding can start here.
    pub keyframe: bool,
}

/// How long to wait for the encoder to ask for more input before giving up.
const NEED_INPUT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

/// A hardware H.264 encoder driven frame by frame.
///
/// Hardware encoders are asynchronous Media Foundation transforms: they
/// announce "I want input" and "I have output" through events, which this
/// type pumps for the caller.
pub struct H264Encoder {
    transform: IMFTransform,
    events: IMFMediaEventGenerator,
    /// Frames the encoder said it can accept.
    need_input: u32,
    frame_len: usize,
    frame_duration: i64,
    provides_samples: bool,
    output_buffer_len: u32,
}

impl H264Encoder {
    /// Opens the first hardware H.264 encoder. Fails when there is none.
    pub fn new(_session: &MfSession, settings: &EncodeSettings) -> Result<Self> {
        let EncodeSettings {
            width,
            height,
            fps,
            bitrate_bps,
        } = *settings;
        let output_info = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_H264,
        };
        // SAFETY: documented MFT setup sequence; every pointer outlives its call.
        unsafe {
            let mut array: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count = 0u32;
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                None,
                Some(&output_info),
                &mut array,
                &mut count,
            )?;
            if array.is_null() || count == 0 {
                return Err(Error::new(E_FAIL, "no hardware H.264 encoder"));
            }
            let slots = std::slice::from_raw_parts_mut(array, count as usize);
            let first = slots.iter_mut().find_map(Option::take);
            for slot in slots.iter_mut() {
                drop(slot.take());
            }
            CoTaskMemFree(Some(array as *const _));
            let activate = first.ok_or_else(|| Error::new(E_FAIL, "no hardware H.264 encoder"))?;
            let transform: IMFTransform = activate.ActivateObject()?;

            // Asynchronous MFTs must be unlocked before use.
            let attributes = transform.GetAttributes()?;
            attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
            let _ = attributes.SetUINT32(&MF_LOW_LATENCY, 1);

            // Output (H.264) first, then input (NV12).
            let out = MFCreateMediaType()?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate_bps)?;
            out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
            out.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
            out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
            transform.SetOutputType(0, &out, 0)?;

            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
            input.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
            input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            transform.SetInputType(0, &input, 0)?;

            // Best-effort tuning: constant bitrate, a keyframe every second so
            // the buffer can be cut cleanly, no B-frames (lower latency).
            if let Ok(codec) = transform.cast::<ICodecAPI>() {
                let set = |key: &windows::core::GUID, value: u32| {
                    let variant = variant_u32(value);
                    let _ = codec.SetValue(key, &variant);
                };
                set(&CODECAPI_AVEncCommonRateControlMode, 0); // CBR
                set(&CODECAPI_AVEncCommonMeanBitRate, bitrate_bps);
                set(&CODECAPI_AVEncMPVGOPSize, fps);
                set(&CODECAPI_AVEncMPVDefaultBPictureCount, 0);
                let _ = codec.SetValue(&CODECAPI_AVLowLatencyMode, &variant_bool(true));
            }

            let info = transform.GetOutputStreamInfo(0)?;
            let provides_samples = info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0;

            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            let events: IMFMediaEventGenerator = transform.cast()?;

            Ok(Self {
                transform,
                events,
                need_input: 0,
                frame_len: nv12_len(width, height),
                frame_duration: frame_duration_100ns(fps),
                provides_samples,
                output_buffer_len: info.cbSize.max(1 << 20),
            })
        }
    }

    /// The compressed stream's media type. After the first frames it carries
    /// the sequence header (SPS/PPS) needed to build an MP4 without
    /// re-encoding.
    pub fn output_type(&self) -> Result<IMFMediaType> {
        // SAFETY: plain query on a configured transform.
        unsafe { self.transform.GetOutputCurrentType(0) }
    }

    /// Feeds one tightly packed NV12 frame and returns every compressed frame
    /// the encoder has produced so far (usually the previous one or two).
    pub fn encode(&mut self, nv12: &[u8], timestamp_100ns: i64) -> Result<Vec<EncodedFrame>> {
        if nv12.len() < self.frame_len {
            return Err(Error::new(
                E_FAIL,
                "NV12 frame is smaller than the configured size",
            ));
        }
        let mut out = Vec::new();
        let deadline = std::time::Instant::now() + NEED_INPUT_TIMEOUT;
        while self.need_input == 0 {
            self.pump_events(&mut out, false)?;
            if self.need_input == 0 {
                if std::time::Instant::now() > deadline {
                    return Err(Error::new(E_FAIL, "encoder did not ask for more input"));
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }

        // SAFETY: the memory buffer is large enough for `frame_len` bytes and
        // is only written while locked.
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
            self.transform.ProcessInput(0, &sample, 0)?;
        }
        self.need_input -= 1;
        self.pump_events(&mut out, false)?;
        Ok(out)
    }

    /// Finishes the stream: returns the frames still inside the encoder.
    pub fn flush(&mut self) -> Result<Vec<EncodedFrame>> {
        let mut out = Vec::new();
        // SAFETY: plain message to a streaming transform.
        unsafe {
            self.transform
                .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)?;
        }
        self.pump_events(&mut out, true)?;
        Ok(out)
    }

    /// Handles queued events: counts input requests and collects outputs.
    /// With `until_drained`, waits for the end-of-drain event.
    fn pump_events(&mut self, out: &mut Vec<EncodedFrame>, until_drained: bool) -> Result<()> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            // SAFETY: GetEvent with NO_WAIT never blocks.
            let event = unsafe { self.events.GetEvent(MF_EVENT_FLAG_NO_WAIT) };
            let event = match event {
                Ok(event) => event,
                Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => {
                    if !until_drained {
                        return Ok(());
                    }
                    if std::time::Instant::now() > deadline {
                        return Err(Error::new(E_FAIL, "encoder drain timed out"));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }
                Err(e) => return Err(e),
            };
            // SAFETY: the event is a valid media event.
            let kind = unsafe { event.GetType()? } as i32;
            if kind == METransformNeedInput.0 {
                self.need_input += 1;
            } else if kind == METransformHaveOutput.0 {
                if let Some(frame) = self.take_output()? {
                    out.push(frame);
                }
            } else if kind == METransformDrainComplete.0 {
                return Ok(());
            }
        }
    }

    fn take_output(&mut self) -> Result<Option<EncodedFrame>> {
        // SAFETY: standard ProcessOutput call. When the transform does not
        // provide samples we supply one; the output structure's COM pointers
        // are released through ManuallyDrop::take exactly once.
        unsafe {
            let mut buffer = MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: std::mem::ManuallyDrop::new(None),
                dwStatus: 0,
                pEvents: std::mem::ManuallyDrop::new(None),
            };
            if !self.provides_samples {
                let sample = MFCreateSample()?;
                sample.AddBuffer(&MFCreateMemoryBuffer(self.output_buffer_len)?)?;
                buffer.pSample = std::mem::ManuallyDrop::new(Some(sample));
            }
            let mut status = 0u32;
            let result =
                self.transform
                    .ProcessOutput(0, std::slice::from_mut(&mut buffer), &mut status);
            let sample = std::mem::ManuallyDrop::take(&mut buffer.pSample);
            drop(std::mem::ManuallyDrop::take(&mut buffer.pEvents));
            match result {
                Ok(()) => {}
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
                Err(e) => return Err(e),
            }
            let Some(sample) = sample else {
                return Ok(None);
            };
            let media = sample.ConvertToContiguousBuffer()?;
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            media.Lock(&mut data, None, Some(&mut len))?;
            let bytes = std::slice::from_raw_parts(data, len as usize).to_vec();
            media.Unlock()?;
            Ok(Some(EncodedFrame {
                data: bytes,
                timestamp: sample.GetSampleTime()?,
                duration: sample.GetSampleDuration().unwrap_or(self.frame_duration),
                keyframe: sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0,
            }))
        }
    }
}

impl Drop for H264Encoder {
    fn drop(&mut self) {
        // SAFETY: tells the encoder the stream is over; errors are irrelevant.
        unsafe {
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
        }
    }
}

// ===========================================================================
// Writing already-compressed frames to an MP4 (no re-encoding).
// ===========================================================================

/// Writes [`EncodedFrame`]s into an MP4 without touching the pixels.
pub struct Mp4Remuxer {
    writer: IMFSinkWriter,
    stream: u32,
    /// Stream indexes of the AAC audio tracks, in track order.
    audio_streams: Vec<u32>,
    /// Timestamp of the first frame, subtracted so the file starts at zero.
    origin: Option<i64>,
    /// The newest frame, held back until the next one shows how long it lasts.
    held: Option<EncodedFrame>,
}

impl Mp4Remuxer {
    /// `sequence_header` is the encoder's SPS/PPS blob, if it exposes one.
    /// `audio_tracks` is how many AAC tracks the file gets (0 for none).
    /// `fragmented` writes a fragmented MP4, which stays valid if the program
    /// stops halfway (used for long recordings).
    pub fn create(
        _session: &MfSession,
        path: &Path,
        settings: &EncodeSettings,
        sequence_header: Option<&[u8]>,
        audio_tracks: usize,
        fragmented: bool,
    ) -> Result<Self> {
        let EncodeSettings {
            width,
            height,
            fps,
            bitrate_bps,
        } = *settings;
        // SAFETY: documented sink-writer setup with a matching input and
        // output type, which makes it pass the samples through untouched.
        unsafe {
            let mut attrs: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attrs, 2)?;
            let attrs = attrs.ok_or_else(|| Error::new(E_FAIL, "no attribute store"))?;
            attrs.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
            if fragmented {
                attrs.SetGUID(
                    &MF_TRANSCODE_CONTAINERTYPE,
                    &MFTranscodeContainerType_FMPEG4,
                )?;
            }
            let url = HSTRING::from(path.as_os_str());
            let writer = MFCreateSinkWriterFromURL(&url, None, &attrs)?;

            let ty = MFCreateMediaType()?;
            ty.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            ty.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            ty.SetUINT32(&MF_MT_AVG_BITRATE, bitrate_bps)?;
            ty.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            ty.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
            ty.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
            ty.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            ty.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
            if let Some(header) = sequence_header {
                ty.SetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, header)?;
            }
            let stream = writer.AddStream(&ty)?;
            writer.SetInputMediaType(stream, &ty, None::<&IMFAttributes>)?;

            // Audio: 16-bit stereo PCM in, AAC (192 kbps) out; the sink writer
            // inserts the encoder.
            let mut audio_streams = Vec::with_capacity(audio_tracks);
            for _ in 0..audio_tracks {
                let rate = crate::audio_sync::SAMPLE_RATE;
                let channels = crate::audio_sync::CHANNELS as u32;
                let aac = MFCreateMediaType()?;
                aac.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                aac.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
                aac.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                aac.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
                aac.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
                aac.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000)?;
                aac.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
                aac.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
                let index = writer.AddStream(&aac)?;

                let pcm = MFCreateMediaType()?;
                pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
                pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
                pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
                pcm.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, channels * 2)?;
                pcm.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * channels * 2)?;
                pcm.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
                writer.SetInputMediaType(index, &pcm, None::<&IMFAttributes>)?;
                audio_streams.push(index);
            }

            writer.BeginWriting()?;
            Ok(Self {
                writer,
                stream,
                audio_streams,
                origin: None,
                held: None,
            })
        }
    }

    /// Adds a frame. The MP4 writer ignores gaps between samples and would
    /// squeeze them out, leaving the audio (which keeps its own timeline)
    /// ahead of the picture; so a frame followed by a long gap is stretched to
    /// last until the next one (the picture freezes during an outage).
    pub fn write(&mut self, frame: &EncodedFrame) -> Result<()> {
        self.origin.get_or_insert(frame.timestamp);
        let previous = self.held.replace(frame.clone());
        match previous {
            Some(mut previous) => {
                let gap = frame.timestamp - previous.timestamp;
                previous.duration = stretched_duration(previous.duration, gap);
                self.write_sample(&previous)
            }
            None => Ok(()),
        }
    }

    fn write_sample(&mut self, frame: &EncodedFrame) -> Result<()> {
        let origin = self.origin.unwrap_or(frame.timestamp);
        // SAFETY: the buffer is filled while locked and sized to the data.
        unsafe {
            let buffer = MFCreateMemoryBuffer(frame.data.len() as u32)?;
            let mut data: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            std::ptr::copy_nonoverlapping(frame.data.as_ptr(), data, frame.data.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(frame.data.len() as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(frame.timestamp - origin)?;
            sample.SetSampleDuration(frame.duration)?;
            if frame.keyframe {
                sample.SetUINT32(&MFSampleExtension_CleanPoint, 1)?;
            }
            self.writer.WriteSample(self.stream, &sample)
        }
    }

    /// Writes interleaved 16-bit stereo PCM to audio track `track` (0-based),
    /// starting `start_100ns` after the beginning of the clip (the same zero as
    /// the video). A no-op when the file has no such track.
    pub fn write_audio(&mut self, track: usize, start_100ns: i64, pcm: &[i16]) -> Result<()> {
        let Some(&stream) = self.audio_streams.get(track) else {
            return Ok(());
        };
        if pcm.is_empty() {
            return Ok(());
        }
        let bytes = pcm.len() * 2;
        let frames = (pcm.len() / crate::audio_sync::CHANNELS) as i64;
        // SAFETY: the buffer is filled while locked and sized to the data.
        unsafe {
            let buffer = MFCreateMemoryBuffer(bytes as u32)?;
            let mut data: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            std::ptr::copy_nonoverlapping(pcm.as_ptr().cast::<u8>(), data, bytes);
            buffer.Unlock()?;
            buffer.SetCurrentLength(bytes as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(start_100ns)?;
            sample.SetSampleDuration(crate::pcm::frames_to_ticks(frames))?;
            self.writer.WriteSample(stream, &sample)
        }
    }

    pub fn finish(mut self) -> Result<()> {
        if let Some(last) = self.held.take() {
            self.write_sample(&last)?;
        }
        // SAFETY: the writer was started with BeginWriting.
        unsafe { self.writer.Finalize() }
    }
}

/// How long a frame should last given the time until the next one: its own
/// duration normally, the whole gap when the next frame is clearly late.
fn stretched_duration(duration: i64, gap_to_next: i64) -> i64 {
    if gap_to_next > duration * 3 / 2 {
        gap_to_next
    } else {
        duration
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn a_frame_before_a_long_gap_is_stretched_over_it() {
        use super::stretched_duration;
        let frame = 333_333; // 30 fps
        assert_eq!(stretched_duration(frame, frame), frame, "on time");
        assert_eq!(
            stretched_duration(frame, frame + 20_000),
            frame,
            "jitter is ignored"
        );
        assert_eq!(
            stretched_duration(frame, 30_000_000),
            30_000_000,
            "a 3 s outage"
        );
    }

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
