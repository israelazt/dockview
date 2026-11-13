//! Writing NV12 frames to an H.264 MP4 through the sink writer.

use super::*;

/// Writes NV12 frames to an H.264 MP4 file.
pub struct Mp4Writer {
    writer: IMFSinkWriter,
    stream: u32,
    frame_len: usize,
    frame_duration: i64,
}

pub(super) fn pack(high: u32, low: u32) -> u64 {
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
