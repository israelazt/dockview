//! Lists the tracks of a media file and how much of each can be read, without
//! decoding anything. Useful to see what survived a crashed recording.
//!
//! Run with: `cargo run --release -p dockview-core --example file_check -- <file.mp4>`

use dockview_core::mf::MfSession;
use windows::core::HSTRING;
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFSample, MFCreateSourceReaderFromURL, MF_MT_MAJOR_TYPE,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ALL_STREAMS,
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
            let kind = if major.to_u128() == 0x73646976_0000_0010_8000_00AA00389B71 {
                "video"
            } else {
                "audio"
            };
            let (mut samples, mut first, mut last, mut bytes) = (0u64, None, 0i64, 0u64);
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
                }
            }
            println!(
                "stream {stream}: {kind}  {samples} samples  {:.2}s..{:.2}s  {:.1} MB",
                first.unwrap_or(0) as f64 / 1e7,
                last as f64 / 1e7,
                bytes as f64 / 1e6
            );
        }
    }
    Ok(())
}
