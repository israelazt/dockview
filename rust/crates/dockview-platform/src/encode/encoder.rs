//! Driving the hardware H.264 encoder frame by frame.

use super::*;

/// How long to wait for the encoder to ask for more input before giving up.
pub(super) const NEED_INPUT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

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
    pub(super) fn pump_events(
        &mut self,
        out: &mut Vec<EncodedFrame>,
        until_drained: bool,
    ) -> Result<()> {
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

    pub(super) fn take_output(&mut self) -> Result<Option<EncodedFrame>> {
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
