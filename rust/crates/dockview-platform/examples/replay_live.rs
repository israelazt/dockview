//! Records a real capture device into the replay buffer for a while, then
//! saves the last few seconds and checks the file.
//!
//! Run with: `cargo run --release -p dockview-core --example replay_live [record_seconds] [clip_seconds] [name_filter] [mbps]`

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dockview_domain::device::match_audio_to_video;
use dockview_domain::frame::FrameSlot;
use dockview_domain::recording::{RecorderStatus, ReplaySettings};
use dockview_domain::sources::CaptureOptions;
use dockview_platform::audio::{enumerate_audio_devices, AudioEngine};
use dockview_platform::capture::VideoCapture;
use dockview_platform::mf::{enumerate_video_devices, MfSession};
use dockview_platform::recorder::{ReplayRecorder, SaveHandleExt};

use windows::core::HSTRING;
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSample, MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL,
    MFMediaType_Video, MFVideoFormat_RGB32, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM,
};

fn main() -> windows::core::Result<()> {
    let mut args = std::env::args().skip(1);
    let record_seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(25);
    let clip_seconds: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let filter = args
        .next()
        .unwrap_or_else(|| "ugreen".into())
        .to_lowercase();
    let mbps: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(40);

    let devices = {
        let session = MfSession::start()?;
        enumerate_video_devices(&session)?
    };
    let Some(device) = devices
        .iter()
        .find(|d| d.friendly_name.to_lowercase().contains(&filter))
    else {
        println!("No video device matching '{filter}'.");
        return Ok(());
    };
    println!("Device: {}", device.friendly_name);

    let slot = Arc::new(FrameSlot::new());
    let capture = VideoCapture::start(
        device.symbolic_link.clone(),
        CaptureOptions::default(),
        Arc::clone(&slot),
    );
    // Game audio from the capture card's own audio endpoint (muted: we only
    // want to record it, not play it).
    let audio_devices = enumerate_audio_devices().unwrap_or_default();
    let audio_device = match_audio_to_video(Some(device), &audio_devices);
    println!(
        "Audio: {}",
        audio_device.map_or("none found".to_string(), |d| d.friendly_name.clone())
    );
    let audio = audio_device.map(|d| AudioEngine::start(d.device_id.clone(), 0.0, 0));
    let recorder = ReplayRecorder::start(
        Arc::clone(&slot),
        ReplaySettings {
            keep_seconds: 30.0,
            bitrate_mbps: mbps,
            fps: 60,
        },
    );

    if let Some(audio) = &audio {
        audio.set_tap(Some(recorder.audio_tap()));
    }

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(record_seconds) {
        thread::sleep(Duration::from_secs(1));
        println!(
            "t={:>2}s capture={:?}  recorder={:?}  video={:>5.1}s audio={:>5.1}s {:>6.1} MB  encoded={}  dropped={}",
            start.elapsed().as_secs(),
            matches!(capture.status(), dockview_domain::sources::CaptureStatus::Running { .. }),
            recorder.status(),
            recorder.buffered_seconds(),
            recorder.buffered_audio_seconds(),
            recorder.buffered_bytes() as f64 / 1e6,
            recorder.encoded_frames(),
            recorder.dropped_frames(),
        );
    }
    if recorder.status() != RecorderStatus::Recording {
        println!("Recorder is not recording: {:?}", recorder.status());
        return Ok(());
    }

    let path = std::env::temp_dir().join("dockview-live-clip.mp4");
    let t = Instant::now();
    let saved = recorder
        .save(clip_seconds, path.clone())
        .wait(Duration::from_secs(60))
        .expect("save finished");
    match saved {
        Ok(clip) => {
            println!(
                "Saved {} frames, {:.2}s, {:.1} MB ({:.1} Mbps) in {:.2}s",
                clip.frames,
                clip.seconds,
                clip.bytes as f64 / 1e6,
                clip.bytes as f64 * 8.0 / clip.seconds / 1e6,
                t.elapsed().as_secs_f64()
            );
            verify(&path, clip.frames)?;
        }
        Err(e) => println!("Save failed: {e}"),
    }
    Ok(())
}

/// Counts the samples and measures the average brightness of a middle frame
/// (so "all black" would be noticed) without looking at the picture.
fn verify(path: &std::path::Path, expected: usize) -> windows::core::Result<()> {
    verify_audio(path)?;
    // SAFETY: plain source-reader calls on a file we just wrote.
    unsafe {
        let reader =
            MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None::<&IMFAttributes>)?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let (mut count, mut first_ts, mut last_ts) = (0usize, None, 0i64);
        loop {
            let (mut flags, mut ts) = (0u32, 0i64);
            let mut sample: Option<IMFSample> = None;
            reader.ReadSample(
                stream,
                0,
                None,
                Some(&mut flags),
                Some(&mut ts),
                Some(&mut sample),
            )?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            if sample.is_some() {
                count += 1;
                first_ts.get_or_insert(ts);
                last_ts = ts;
            }
        }
        println!(
            "Read back: {count} samples (saved {expected}), {:.3}s -> {:.3}s",
            first_ts.unwrap_or(0) as f64 / 1e7,
            last_ts as f64 / 1e7
        );

        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1)?;
        let attrs = attrs.expect("attributes");
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), &attrs)?;
        let rgb = MFCreateMediaType()?;
        rgb.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        rgb.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        reader.SetCurrentMediaType(stream, None, &rgb)?;

        let middle = count / 2;
        for i in 0..=middle {
            let (mut flags, mut ts) = (0u32, 0i64);
            let mut sample: Option<IMFSample> = None;
            reader.ReadSample(
                stream,
                0,
                None,
                Some(&mut flags),
                Some(&mut ts),
                Some(&mut sample),
            )?;
            if i == middle {
                let Some(sample) = sample else { break };
                let buffer = sample.ConvertToContiguousBuffer()?;
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut len = 0u32;
                buffer.Lock(&mut data, None, Some(&mut len))?;
                let pixels = std::slice::from_raw_parts(data, len as usize);
                let (mut sum, mut n) = (0u64, 0u64);
                for px in pixels.chunks(4 * 997) {
                    sum += (u64::from(px[0]) + u64::from(px[1]) + u64::from(px[2])) / 3;
                    n += 1;
                }
                buffer.Unlock()?;
                println!(
                    "Middle frame ({:.2}s): average brightness {}/255 over {n} samples",
                    ts as f64 / 1e7,
                    sum / n.max(1)
                );
            }
        }
    }
    Ok(())
}

/// Decodes the audio track: its length and RMS level.
fn verify_audio(path: &std::path::Path) -> windows::core::Result<()> {
    use windows::Win32::Media::MediaFoundation::{
        MFAudioFormat_PCM, MFMediaType_Audio, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
        MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
        MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    };
    // SAFETY: plain source-reader calls on a file we just wrote.
    unsafe {
        let reader =
            MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None::<&IMFAttributes>)?;
        let stream = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
        let pcm = MFCreateMediaType()?;
        pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
        pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48_000)?;
        pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        pcm.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
        pcm.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192_000)?;
        if reader.SetCurrentMediaType(stream, None, &pcm).is_err() {
            println!("Audio track: none");
            return Ok(());
        }
        let (mut frames, mut sum_squares, mut peak) = (0u64, 0f64, 0f64);
        loop {
            let (mut flags, mut ts) = (0u32, 0i64);
            let mut sample: Option<IMFSample> = None;
            reader.ReadSample(
                stream,
                0,
                None,
                Some(&mut flags),
                Some(&mut ts),
                Some(&mut sample),
            )?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
            let Some(sample) = sample else { continue };
            let buffer = sample.ConvertToContiguousBuffer()?;
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            buffer.Lock(&mut data, None, Some(&mut len))?;
            let bytes = std::slice::from_raw_parts(data, len as usize);
            let (pairs, _) = bytes.as_chunks::<2>();
            for pair in pairs {
                let v = f64::from(i16::from_le_bytes(*pair)) / 32768.0;
                sum_squares += v * v;
                peak = peak.max(v.abs());
            }
            frames += u64::from(len / 4);
            buffer.Unlock()?;
        }
        let seconds = frames as f64 / 48_000.0;
        let rms = (sum_squares / (frames.max(1) as f64 * 2.0)).sqrt();
        println!(
            "Audio track: {seconds:.2}s, RMS {rms:.4} ({:.1} dBFS), peak {peak:.3}",
            20.0 * rms.max(1e-9).log10()
        );
    }
    Ok(())
}
