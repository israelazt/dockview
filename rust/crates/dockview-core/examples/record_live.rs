//! Records a real capture device to a file for a while (a normal recording,
//! not the replay buffer) and reports what it cost.
//!
//! Run with: `cargo run --release -p dockview-core --example record_live [seconds] [output.mp4] [name_filter] [mbps] [--mic]`

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dockview_core::audio::{enumerate_audio_devices, AudioEngine};
use dockview_core::capture::{CaptureOptions, VideoCapture};
use dockview_core::device::match_audio_to_video;
use dockview_core::frame::FrameSlot;
use dockview_core::mf::{enumerate_video_devices, MfSession};
use dockview_core::mic::MicCapture;
use dockview_core::recorder::{RecorderStatus, ReplayRecorder, ReplaySettings};
use dockview_core::recording::RecordingState;

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

fn cpu_seconds() -> f64 {
    let (mut a, mut b, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: valid out-pointers for the current process.
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut kernel, &mut user);
    }
    let t = |f: FILETIME| ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as f64;
    (t(kernel) + t(user)) / 1e7
}

fn main() -> windows::core::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let with_mic = args.iter().any(|a| a == "--mic");
    args.retain(|a| a != "--mic");
    let mut args = args.into_iter();
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    let path: PathBuf = args.next().map_or_else(
        || std::env::temp_dir().join("dockview-record-live.mp4"),
        PathBuf::from,
    );
    let filter = args
        .next()
        .unwrap_or_else(|| "ugreen".into())
        .to_lowercase();
    let mbps: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);

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
    let _capture = VideoCapture::start(
        device.symbolic_link.clone(),
        CaptureOptions::default(),
        Arc::clone(&slot),
    );
    let audio_devices = enumerate_audio_devices().unwrap_or_default();
    let audio_device = match_audio_to_video(Some(device), &audio_devices);
    let audio = audio_device.map(|d| AudioEngine::start(d.device_id.clone(), 0.0, 0));
    let recorder = ReplayRecorder::start(
        Arc::clone(&slot),
        ReplaySettings {
            keep_seconds: 10.0,
            bitrate_mbps: mbps,
            fps: 60,
        },
    );
    if let Some(audio) = &audio {
        audio.set_tap(Some(recorder.audio_tap()));
    }
    let _mic = with_mic.then(|| MicCapture::start(None, 1.0, recorder.mic_tap()));

    // Wait for the recorder to be running, then record.
    let ready = Instant::now() + Duration::from_secs(10);
    while recorder.status() != RecorderStatus::Recording && Instant::now() < ready {
        thread::sleep(Duration::from_millis(50));
    }
    println!("Recorder: {:?}", recorder.status());
    if let Err(e) = recorder.start_recording(path.clone()) {
        println!("Could not start: {e}");
        return Ok(());
    }
    let cpu_start = cpu_seconds();
    let started = Instant::now();
    for second in 1..=seconds {
        thread::sleep(Duration::from_secs(1));
        let info = recorder.recording();
        if second % 5 == 0 || second == 1 {
            println!(
                "t={second:>3}s  {:?}  {:>6.1}s written  {:>6.1} MB",
                info.state,
                info.seconds,
                info.bytes as f64 / 1e6
            );
        }
        if info.state == RecordingState::Idle {
            break;
        }
    }
    let cpu = cpu_seconds() - cpu_start;
    let elapsed = started.elapsed().as_secs_f64();

    recorder.stop_recording();
    let deadline = Instant::now() + Duration::from_secs(20);
    let result = loop {
        if let Some(r) = recorder.take_recording_result() {
            break r;
        }
        if Instant::now() > deadline {
            println!("The recording never finished.");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    };
    match result {
        Ok(clip) => println!(
            "\nSaved {}\n  {} frames, {:.2} s ({:.1} fps), {:.1} MB ({:.1} Mbps)\n  CPU of this process while recording: {:.1}% of one core",
            clip.path.display(),
            clip.frames,
            clip.seconds,
            clip.frames as f64 / clip.seconds,
            clip.bytes as f64 / 1e6,
            clip.bytes as f64 * 8.0 / clip.seconds / 1e6,
            cpu / elapsed * 100.0,
        ),
        Err(e) => println!("Recording failed: {e}"),
    }
    Ok(())
}
