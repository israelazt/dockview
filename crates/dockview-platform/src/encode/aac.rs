//! The AAC encoder for a live stream, on its own (the MP4 writer builds its
//! own inside the sink writer): PCM in, raw AAC units out.

use dockview_domain::audio_sync::{CHANNELS, SAMPLE_RATE};
use dockview_domain::broadcast::{audio_specific_config, AudioConfig, AudioEncoder, EncodedAudio};
use windows::Win32::Media::MediaFoundation::{AACMFTEncoder, MF_E_NOTACCEPTING, MF_MT_USER_DATA};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

use super::*;

/// Bits per second of the stream's audio.
const AAC_BYTES_PER_SECOND: u32 = 16_000; // 128 kbit/s
/// PCM frames inside one AAC unit.
const SAMPLES_PER_UNIT: i64 = 1024;
const TICKS_PER_SECOND: i64 = 10_000_000;

pub struct AacEncoder {
    transform: IMFTransform,
    output_len: u32,
    /// Timestamp of the first unit, then advanced a unit at a time: the AAC
    /// transform does not stamp its output.
    next_unit: Option<i64>,
}

impl AacEncoder {
    pub fn new(_session: &MfSession) -> Result<Self> {
        let channels = CHANNELS as u32;
        // SAFETY: documented MFT setup: output type first, then input type.
        unsafe {
            let transform: IMFTransform =
                CoCreateInstance(&AACMFTEncoder, None, CLSCTX_INPROC_SERVER)?;

            let out = MFCreateMediaType()?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
            out.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            out.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE)?;
            out.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
            out.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, AAC_BYTES_PER_SECOND)?;
            out.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?; // raw, no ADTS
            out.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
            transform.SetOutputType(0, &out, 0)?;

            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            input.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
            input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, SAMPLE_RATE)?;
            input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
            input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, channels * 2)?;
            input.SetUINT32(
                &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                SAMPLE_RATE * channels * 2,
            )?;
            transform.SetInputType(0, &input, 0)?;

            let info = transform.GetOutputStreamInfo(0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
            transform.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
            Ok(Self {
                transform,
                output_len: info.cbSize.max(8192),
                next_unit: None,
            })
        }
    }

    /// The AudioSpecificConfig the transform itself reports (after its own
    /// 12-byte header), to check against the one computed by the domain.
    pub fn reported_config(&self) -> Option<Vec<u8>> {
        // SAFETY: plain queries on a configured transform.
        unsafe {
            let ty = self.transform.GetOutputCurrentType(0).ok()?;
            let size = ty.GetBlobSize(&MF_MT_USER_DATA).ok()? as usize;
            let mut blob = vec![0u8; size];
            ty.GetBlob(&MF_MT_USER_DATA, &mut blob, None).ok()?;
            blob.get(12..).map(<[u8]>::to_vec)
        }
    }

    fn feed(&mut self, pcm: &[u8], timestamp: i64) -> Result<()> {
        // SAFETY: the buffer holds `pcm.len()` bytes and is only written locked.
        unsafe {
            let buffer = MFCreateMemoryBuffer(pcm.len() as u32)?;
            let mut data: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            std::ptr::copy_nonoverlapping(pcm.as_ptr(), data, pcm.len());
            buffer.Unlock()?;
            buffer.SetCurrentLength(pcm.len() as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(timestamp)?;
            self.transform.ProcessInput(0, &sample, 0)
        }
    }

    /// Takes every unit the transform has ready.
    fn drain(&mut self, units: &mut Vec<EncodedAudio>) -> Result<()> {
        loop {
            // SAFETY: standard ProcessOutput with a sample we provide; the
            // output structure's COM pointers are released exactly once.
            let bytes = unsafe {
                let sample = MFCreateSample()?;
                sample.AddBuffer(&MFCreateMemoryBuffer(self.output_len)?)?;
                let mut buffer = MFT_OUTPUT_DATA_BUFFER {
                    dwStreamID: 0,
                    pSample: std::mem::ManuallyDrop::new(Some(sample)),
                    dwStatus: 0,
                    pEvents: std::mem::ManuallyDrop::new(None),
                };
                let mut status = 0u32;
                let result =
                    self.transform
                        .ProcessOutput(0, std::slice::from_mut(&mut buffer), &mut status);
                let sample = std::mem::ManuallyDrop::take(&mut buffer.pSample);
                drop(std::mem::ManuallyDrop::take(&mut buffer.pEvents));
                match result {
                    Ok(()) => {}
                    Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                    Err(e) => return Err(e),
                }
                let Some(sample) = sample else {
                    return Ok(());
                };
                let media = sample.ConvertToContiguousBuffer()?;
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut len = 0u32;
                media.Lock(&mut data, None, Some(&mut len))?;
                let bytes = std::slice::from_raw_parts(data, len as usize).to_vec();
                media.Unlock()?;
                bytes
            };
            let step = SAMPLES_PER_UNIT * TICKS_PER_SECOND / i64::from(SAMPLE_RATE);
            let timestamp = self.next_unit.unwrap_or(0);
            self.next_unit = Some(timestamp + step);
            units.push(EncodedAudio {
                data: bytes,
                timestamp,
                duration: step,
            });
        }
    }
}

impl AudioEncoder for AacEncoder {
    fn config(&self) -> AudioConfig {
        AudioConfig {
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS as u8,
            specific_config: audio_specific_config(SAMPLE_RATE, CHANNELS as u8).unwrap_or_default(),
        }
    }

    fn encode(
        &mut self,
        samples: &[f32],
        timestamp: i64,
    ) -> std::result::Result<Vec<EncodedAudio>, String> {
        if self.next_unit.is_none() {
            self.next_unit = Some(timestamp);
        }
        let pcm: Vec<u8> = samples
            .iter()
            .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
            .collect();
        let mut units = Vec::new();
        let mut fed = self.feed(&pcm, timestamp);
        if matches!(&fed, Err(e) if e.code() == MF_E_NOTACCEPTING) {
            // Full of finished units: take them and try once more.
            self.drain(&mut units).map_err(|e| e.message())?;
            fed = self.feed(&pcm, timestamp);
        }
        fed.map_err(|e| e.message())?;
        self.drain(&mut units).map_err(|e| e.message())?;
        Ok(units)
    }
}

impl Drop for AacEncoder {
    fn drop(&mut self) {
        // SAFETY: tells the transform the stream is over; errors are irrelevant.
        unsafe {
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize, from: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let v = (((from + i) as f32) * 440.0 * std::f32::consts::TAU / SAMPLE_RATE as f32)
                    .sin()
                    * 0.3;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn turns_a_second_of_tone_into_about_47_units_in_order() {
        let session = MfSession::start().unwrap();
        let Ok(mut enc) = AacEncoder::new(&session) else {
            eprintln!("no AAC encoder on this PC: skipping");
            return;
        };
        let mut units = Vec::new();
        for block in 0..50 {
            let at = block * 960; // 20 ms blocks
            let ts = at as i64 * TICKS_PER_SECOND / i64::from(SAMPLE_RATE);
            units.extend(enc.encode(&tone(960, at), ts).unwrap());
        }
        assert!((40..=48).contains(&units.len()), "{} units", units.len());
        assert!(units
            .iter()
            .all(|u| !u.data.is_empty() && u.data.len() < 2000));
        assert!(units.windows(2).all(|w| w[1].timestamp > w[0].timestamp));
        assert_eq!(units[0].duration, 213_333);
    }

    #[test]
    fn the_config_matches_what_the_transform_reports() {
        let session = MfSession::start().unwrap();
        let Ok(enc) = AacEncoder::new(&session) else {
            return;
        };
        let config = enc.config();
        assert_eq!(config.specific_config, [0x11, 0x90]);
        if let Some(reported) = enc.reported_config() {
            assert_eq!(&reported[..2], &config.specific_config[..]);
        }
    }
}
