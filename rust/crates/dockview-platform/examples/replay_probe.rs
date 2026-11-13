//! Replay-buffer feasibility probe: encode with direct control of the GPU
//! encoder, keep the compressed frames, then write only the last few seconds
//! to an MP4 *without re-encoding* and check the file by decoding it.
//!
//! Run with: `cargo run --release -p dockview-core --example replay_probe [seconds] [clip_seconds] [mbps]`

use std::path::PathBuf;
use std::time::Instant;

use dockview_platform::encode::{
    frame_duration_100ns, sequence_header, EncodeSettings, EncodedFrame, H264Encoder, Mp4Remuxer,
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
    let clip_seconds: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(4);
    let mbps: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(40);
    let path: PathBuf = std::env::temp_dir().join("dockview-replay-probe.mp4");

    let session = MfSession::start()?;
    let settings = EncodeSettings {
        width: WIDTH,
        height: HEIGHT,
        fps: FPS,
        bitrate_bps: mbps * 1_000_000,
    };

    let t_new = Instant::now();
    let mut encoder = H264Encoder::new(&session, &settings)?;
    println!(
        "H264Encoder::new took {:.0} ms",
        t_new.elapsed().as_secs_f64() * 1000.0
    );
    let base = nv12_bars(WIDTH, HEIGHT);
    let mut frame = base.clone();
    let mut encoded: Vec<EncodedFrame> = Vec::new();

    let start = Instant::now();
    for i in 0..seconds * FPS {
        frame.data.copy_from_slice(&base.data);
        overlay_stripe(&mut frame, u64::from(i));
        let t_frame = Instant::now();
        let out = encoder.encode(&frame.data, i64::from(i) * frame_duration_100ns(FPS))?;
        if i < 4 || (!out.is_empty() && encoded.is_empty()) {
            println!(
                "frame {i}: encode() {:.1} ms, returned {} frames",
                t_frame.elapsed().as_secs_f64() * 1000.0,
                out.len()
            );
        }
        encoded.extend(out);
    }
    encoded.extend(encoder.flush()?);
    let took = start.elapsed();

    let keyframes: Vec<f64> = encoded
        .iter()
        .filter(|f| f.keyframe)
        .map(|f| f.timestamp as f64 / 1e7)
        .collect();
    let bytes: usize = encoded.iter().map(|f| f.data.len()).sum();
    println!(
        "Encoded {} of {} frames in {:.2}s ({:.1}x real time)",
        encoded.len(),
        seconds * FPS,
        took.as_secs_f64(),
        f64::from(seconds) / took.as_secs_f64()
    );
    println!(
        "Keyframes: {} at {:?}",
        keyframes.len(),
        keyframes
            .iter()
            .map(|t| format!("{t:.2}"))
            .collect::<Vec<_>>()
    );
    println!(
        "Compressed size: {:.2} MB ({:.1} Mbps)",
        bytes as f64 / 1e6,
        bytes as f64 * 8.0 / f64::from(seconds) / 1e6
    );
    let ordered = encoded.windows(2).all(|w| w[0].timestamp < w[1].timestamp);
    println!("Timestamps strictly increasing: {ordered}");
    let header = sequence_header(&encoder.output_type()?);
    println!(
        "Sequence header (SPS/PPS) from the encoder: {}",
        header
            .as_ref()
            .map_or("none".into(), |h| format!("{} bytes", h.len()))
    );
    println!(
        "First frame starts with Annex B start code: {}",
        encoded
            .first()
            .is_some_and(|f| f.data.starts_with(&[0, 0, 0, 1]) || f.data.starts_with(&[0, 0, 1]))
    );

    println!("NAL types, frame 0:   {:?}", nal_types(&encoded[0].data));
    println!("NAL types, frame 1:   {:?}", nal_types(&encoded[1].data));
    println!(
        "NAL types, frame 60:  {:?}  (a later keyframe)",
        nal_types(&encoded[60].data)
    );
    // Keep only the last `clip_seconds`, starting at a keyframe.
    let newest = encoded.last().map_or(0, |f| f.timestamp);
    let wanted_start = newest - i64::from(clip_seconds) * 10_000_000;
    let first = encoded
        .iter()
        .position(|f| f.keyframe && f.timestamp >= wanted_start)
        .unwrap_or(0);
    let clip = &encoded[first..];
    println!(
        "Clip: frames {first}..{} ({:.2}s of video, starts at {:.2}s)",
        encoded.len(),
        clip.len() as f64 / f64::from(FPS),
        clip[0].timestamp as f64 / 1e7
    );

    let mut remux = Mp4Remuxer::create(&session, &path, &settings, header.as_deref(), 0, false)?;
    for f in clip {
        remux.write(f)?;
    }
    remux.finish()?;
    println!(
        "Wrote {} ({:.2} MB) without re-encoding",
        path.display(),
        std::fs::metadata(&path)?.len() as f64 / 1e6
    );

    verify(&path, clip.len(), first as u64)
}

/// H.264 NAL unit types found in an Annex B access unit (7 = SPS, 8 = PPS,
/// 5 = IDR slice, 1 = non-IDR slice, 6 = SEI, 9 = delimiter).
fn nal_types(data: &[u8]) -> Vec<u8> {
    let mut types = Vec::new();
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            types.push(data[i + 3] & 0x1F);
            i += 3;
        } else {
            i += 1;
        }
    }
    types
}

/// Decodes the file: counts samples and checks the first frame's colours.
fn verify(
    path: &std::path::Path,
    expected_frames: usize,
    first_tick: u64,
) -> windows::core::Result<()> {
    // SAFETY: plain source-reader calls on a file we just wrote.
    unsafe {
        let reader =
            MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None::<&IMFAttributes>)?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let (mut count, mut last_ts, mut first_ts) = (0usize, 0i64, None);
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
                first_ts.get_or_insert(ts);
                last_ts = ts;
            }
        }
        println!(
            "Read back: {count} samples (expected {expected_frames}), first {:.3}s, last {:.3}s",
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
        let buffer = sample.expect("first sample").ConvertToContiguousBuffer()?;
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        buffer.Lock(&mut data, None, Some(&mut len))?;
        let pixels = std::slice::from_raw_parts(data, len as usize);
        let stride = pixels.len() / HEIGHT as usize;
        let want: [[u8; 3]; 8] = [
            [255, 255, 255],
            [255, 255, 0],
            [0, 255, 255],
            [0, 255, 0],
            [255, 0, 255],
            [255, 0, 0],
            [0, 0, 255],
            [0, 0, 0],
        ];
        // The moving white stripe (and its neutral chroma) covers part of the
        // picture; skip bars it overlaps. Same formula as the generator.
        let stripe_w = (WIDTH as usize / 16).max(2);
        let stripe_x = (first_tick as usize * 12) % (WIDTH as usize - stripe_w).max(1);
        let covered = |x: usize| x + 4 >= stripe_x && x <= stripe_x + stripe_w + 6;
        let (mut worst, mut checked) = (0i32, 0);
        for (i, w) in want.iter().enumerate() {
            let x = (WIDTH as usize * (2 * i + 1)) / 16;
            if covered(x) {
                continue;
            }
            checked += 1;
            let at = (HEIGHT as usize / 4) * stride + x * 4;
            let (b, g, r) = (pixels[at], pixels[at + 1], pixels[at + 2]);
            for (got, exp) in [(r, w[0]), (g, w[1]), (b, w[2])] {
                worst = worst.max((i32::from(got) - i32::from(exp)).abs());
            }
        }
        buffer.Unlock()?;
        println!("Decoded first frame: worst colour error {worst}/255 over {checked} bars (stripe at x={stripe_x})");
    }
    Ok(())
}
