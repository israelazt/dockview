//! Encodes the synthetic 1080p60 pattern to an MP4 as fast as the encoder
//! allows, then reads the file back and reports what came out.
//!
//! Run with: `cargo run --release -p dockview-core --example encode_probe [seconds] [mbps] [file.mp4]`

use std::path::PathBuf;
use std::time::Instant;

use dockview_platform::encode::{
    frame_duration_100ns, hardware_h264_encoders, EncodeSettings, Mp4Writer,
};
use dockview_platform::mf::MfSession;
use dockview_platform::testsrc::{nv12_bars, overlay_stripe};

use windows::core::HSTRING;
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSample, MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL,
    MFMediaType_Video, MFVideoFormat_RGB32, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM,
};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const FPS: u32 = 60;

fn main() -> windows::core::Result<()> {
    let mut args = std::env::args().skip(1);
    let seconds: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
    let mbps: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(40);
    let path: PathBuf = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("dockview-probe.mp4"));

    let session = MfSession::start()?;
    let encoders = hardware_h264_encoders(&session)?;
    if encoders.is_empty() {
        println!("Hardware H.264 encoders: none (Windows will use its software encoder)");
    } else {
        println!("Hardware H.264 encoders: {}", encoders.join(", "));
    }

    let settings = EncodeSettings {
        width: WIDTH,
        height: HEIGHT,
        fps: FPS,
        bitrate_bps: mbps * 1_000_000,
    };
    let frames = seconds * FPS;
    println!(
        "Encoding {frames} frames of {WIDTH}x{HEIGHT}@{FPS} at {mbps} Mbps -> {}",
        path.display()
    );

    let base = nv12_bars(WIDTH, HEIGHT);
    let mut frame = base.clone();
    let mut writer = Mp4Writer::create(&session, &path, &settings)?;

    let start = Instant::now();
    for i in 0..frames {
        frame.data.copy_from_slice(&base.data);
        overlay_stripe(&mut frame, u64::from(i));
        writer.write_nv12(&frame.data, i64::from(i) * frame_duration_100ns(FPS))?;
    }
    let submitted = start.elapsed();
    writer.finish()?;
    let total = start.elapsed();

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let clip_secs = f64::from(seconds);
    println!(
        "Submitted in {:.2}s, finished in {:.2}s -> {:.1}x real time ({:.0} fps)",
        submitted.as_secs_f64(),
        total.as_secs_f64(),
        clip_secs / total.as_secs_f64(),
        f64::from(frames) / total.as_secs_f64()
    );
    println!(
        "File: {:.1} MB ({:.1} Mbps average)",
        size as f64 / 1e6,
        size as f64 * 8.0 / clip_secs / 1e6
    );

    // Read it back: count the compressed samples and look at the timestamps.
    // SAFETY: plain source-reader calls on a file we just wrote.
    unsafe {
        let reader =
            MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None::<&IMFAttributes>)?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let (mut count, mut last_ts) = (0u32, 0i64);
        loop {
            let mut flags = 0u32;
            let mut ts = 0i64;
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
                last_ts = ts;
            }
        }
        println!(
            "Read back: {count} samples, last timestamp {:.3}s (expected ~{:.3}s)",
            last_ts as f64 / 1e7,
            clip_secs - 1.0 / f64::from(FPS)
        );
    }
    // Decode the first frame to RGB and compare it with the colour bars we fed in.
    // SAFETY: plain source-reader calls on a file we just wrote.
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1)?;
        let attrs = attrs.expect("attributes");
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), &attrs)?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let rgb = MFCreateMediaType()?;
        rgb.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        rgb.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        reader.SetCurrentMediaType(stream, None, &rgb)?;

        let mut sample: Option<IMFSample> = None;
        let (mut flags, mut ts) = (0u32, 0i64);
        reader.ReadSample(
            stream,
            0,
            None,
            Some(&mut flags),
            Some(&mut ts),
            Some(&mut sample),
        )?;
        let sample = sample.expect("first sample");
        let buffer = sample.ConvertToContiguousBuffer()?;
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        buffer.Lock(&mut data, None, Some(&mut len))?;
        let pixels = std::slice::from_raw_parts(data, len as usize);

        // RGB32 from the video processor may be bottom-up; probe both rows.
        let stride = pixels.len() / HEIGHT as usize;
        let expected: [(&str, [u8; 3]); 8] = [
            ("white", [255, 255, 255]),
            ("yellow", [255, 255, 0]),
            ("cyan", [0, 255, 255]),
            ("green", [0, 255, 0]),
            ("magenta", [255, 0, 255]),
            ("red", [255, 0, 0]),
            ("blue", [0, 0, 255]),
            ("black", [0, 0, 0]),
        ];
        let mut worst = 0i32;
        println!("Decoded first frame (centre row, RGB):");
        for (i, (name, want)) in expected.iter().enumerate() {
            let x = (WIDTH as usize * (2 * i + 1)) / 16;
            let row = HEIGHT as usize / 4; // away from the moving stripe at the start
            let at = row * stride + x * 4;
            let (b, g, r) = (pixels[at], pixels[at + 1], pixels[at + 2]);
            let diff = (i32::from(r) - i32::from(want[0]))
                .abs()
                .max((i32::from(g) - i32::from(want[1])).abs())
                .max((i32::from(b) - i32::from(want[2])).abs());
            worst = worst.max(diff);
            println!("  {name:<8} got ({r:>3},{g:>3},{b:>3})  expected {want:?}  off by {diff}");
        }
        println!("Worst channel error: {worst} (lossy compression allows a few units)");
        buffer.Unlock()?;
    }
    Ok(())
}
