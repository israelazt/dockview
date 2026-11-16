//! Lists the tracks of a media file and how much of each can be read, with the
//! loudness of every audio track. Useful to see what survived a crashed
//! recording, and whether the right sound was recorded.
//!
//! Run with: `cargo run --release -p dockview-core --example file_check -- <file.mp4>`

use dockview_platform::mf::MfSession;
use windows::core::HSTRING;
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSample, MFAudioFormat_PCM, MFCreateMediaType, MFCreateSourceReaderFromURL,
    MFMediaType_Audio, MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE,
    MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
    MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ALL_STREAMS,
};

fn main() -> windows::core::Result<()> {
    let Some(path) = std::env::args().nth(1) else {
        println!("usage: file_check <file>");
        return Ok(());
    };
    let _session = MfSession::start()?;
    for stream in 0..8u32 {
        // SAFETY: plain source-reader calls on a file the user named.
        unsafe {
            let reader =
                MFCreateSourceReaderFromURL(&HSTRING::from(path.as_str()), None::<&IMFAttributes>)?;
            let Ok(native) = reader.GetNativeMediaType(stream, 0) else {
                continue;
            };
            // Read this track alone, or the others hold the reader back.
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
            reader.SetStreamSelection(stream, true)?;
            let major = native.GetGUID(&MF_MT_MAJOR_TYPE)?;
            let audio = major == MFMediaType_Audio;
            if audio {
                // Ask for 16-bit stereo PCM so the sound can be measured.
                let pcm = MFCreateMediaType()?;
                pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
                pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
                pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)?;
                pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                pcm.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
                pcm.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192_000)?;
                let _ = reader.SetCurrentMediaType(stream, None, &pcm);
            }
            let (mut samples, mut first, mut last, mut bytes) = (0u64, None, 0i64, 0u64);
            let (mut squares, mut count, mut peak) = (0f64, 0u64, 0f64);
            loop {
                let (mut flags, mut ts) = (0u32, 0i64);
                let mut sample: Option<IMFSample> = None;
                if reader
                    .ReadSample(
                        stream,
                        0,
                        None,
                        Some(&mut flags),
                        Some(&mut ts),
                        Some(&mut sample),
                    )
                    .is_err()
                {
                    break;
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
                if let Some(sample) = sample {
                    samples += 1;
                    first.get_or_insert(ts);
                    last = ts + sample.GetSampleDuration().unwrap_or(0);
                    bytes += u64::from(sample.GetTotalLength().unwrap_or(0));
                    if audio {
                        let buffer = sample.ConvertToContiguousBuffer()?;
                        let mut data: *mut u8 = std::ptr::null_mut();
                        let mut len = 0u32;
                        buffer.Lock(&mut data, None, Some(&mut len))?;
                        let (pairs, _) =
                            std::slice::from_raw_parts(data, len as usize).as_chunks::<2>();
                        for pair in pairs {
                            let v = f64::from(i16::from_le_bytes(*pair)) / 32768.0;
                            squares += v * v;
                            peak = peak.max(v.abs());
                            count += 1;
                        }
                        buffer.Unlock()?;
                    }
                }
            }
            let kind = if audio { "audio" } else { "video" };
            let level = if audio && count > 0 {
                format!(
                    "  RMS {:.4}  peak {:.3}",
                    (squares / count as f64).sqrt(),
                    peak
                )
            } else {
                String::new()
            };
            println!(
                "stream {stream}: {kind}  {samples} samples  {:.2}s..{:.2}s  {:.1} MB{level}",
                first.unwrap_or(0) as f64 / 1e7,
                last as f64 / 1e7,
                bytes as f64 / 1e6
            );
        }
    }
    Ok(())
}
