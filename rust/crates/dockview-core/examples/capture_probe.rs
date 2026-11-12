//! Captures from the first video device for a few seconds and reports fps,
//! dropped frames and timestamp behaviour.
//!
//! Run with: `cargo run -p dockview-core --example capture_probe [seconds] [mode_index] [name_filter]`

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dockview_core::capture::{CaptureOptions, CaptureStatus, VideoCapture};
use dockview_core::frame::FrameSlot;
use dockview_core::mf::{enumerate_video_devices, MfSession};

fn main() -> windows::core::Result<()> {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);

    let mode_index: Option<u32> = std::env::args().nth(2).and_then(|s| s.parse().ok());

    let devices = {
        let session = MfSession::start()?;
        enumerate_video_devices(&session)?
    };
    let filter = std::env::args().nth(3).map(|f| f.to_lowercase());
    let device = devices.iter().find(|d| {
        filter
            .as_deref()
            .is_none_or(|f| d.friendly_name.to_lowercase().contains(f))
    });
    let Some(device) = device else {
        println!("No video capture devices found.");
        return Ok(());
    };
    println!("Device: {}", device.friendly_name);

    let slot = Arc::new(FrameSlot::new());
    let capture = VideoCapture::start(
        device.symbolic_link.clone(),
        CaptureOptions {
            mode_index,
            ..CaptureOptions::default()
        },
        Arc::clone(&slot),
    );

    let start = Instant::now();
    let mut consumed = 0u64;
    let mut last_report = Instant::now();
    let mut last_published = 0u64;
    let mut first_ts: Option<i64> = None;
    let mut last_frame_info = String::new();

    while start.elapsed() < Duration::from_secs(seconds) {
        if let Some(frame) = slot.take_latest() {
            consumed += 1;
            first_ts.get_or_insert(frame.timestamp);
            last_frame_info = format!(
                "{}x{} stride {} {:?}, {} bytes",
                frame.width,
                frame.height,
                frame.stride,
                frame.format,
                frame.data.len()
            );
            slot.recycle(frame);
        }

        if last_report.elapsed() >= Duration::from_secs(1) {
            let published = slot.published();
            println!(
                "t={:>2}s status={:?} fps={} dropped={} consumed={consumed}",
                start.elapsed().as_secs(),
                capture.status(),
                published - last_published,
                slot.dropped(),
            );
            last_published = published;
            last_report = Instant::now();
        }
        thread::sleep(Duration::from_millis(2));
    }

    println!("\nFrame: {last_frame_info}");
    println!("Total published: {}", slot.published());
    if !matches!(capture.status(), CaptureStatus::Running { .. }) {
        println!("Final status: {:?}", capture.status());
    }
    Ok(())
}
