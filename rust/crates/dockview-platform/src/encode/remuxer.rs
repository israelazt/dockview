//! Writing already-compressed frames to an MP4, without encoding again.

use super::*;

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
                let rate = dockview_domain::audio_sync::SAMPLE_RATE;
                let channels = dockview_domain::audio_sync::CHANNELS as u32;
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

    pub(super) fn write_sample(&mut self, frame: &EncodedFrame) -> Result<()> {
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
        let frames = (pcm.len() / dockview_domain::audio_sync::CHANNELS) as i64;
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
            sample.SetSampleDuration(dockview_domain::pcm::frames_to_ticks(frames))?;
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
pub(super) fn stretched_duration(duration: i64, gap_to_next: i64) -> i64 {
    if gap_to_next > duration * 3 / 2 {
        gap_to_next
    } else {
        duration
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
