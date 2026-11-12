//! Records a monitor or a window to a file, with the PC's or one program's
//! sound and optionally the microphone, and reports what it cost.
//!
//! Run with:
//! `cargo run --release -p dockview-core --example screen_record -- [options]`
//!
//!   --window <text>   capture the first window whose title contains <text>
//!                     (`=Title` must match exactly); default is a monitor
//!   --monitor <n>     which monitor (0 is the primary); default 0
//!   --audio <mode>    `system`, `process` (only the window's program) or
//!                     `none`; default is `process` with --window, else `system`
//!   --mic             also record the microphone
//!   --seconds <n>     how long (default 15)
//!   --out <file>      output file (default: a temporary file)
//!   --dump <file>     write the first captured frame as raw NV12 for checking
//!   --mbps <n>        video bitrate (default 20)

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dockview_core::frame::FrameSlot;
use dockview_core::recorder::{RecorderStatus, ReplayRecorder, ReplaySettings};
use dockview_core::recording::RecordingState;
use dockview_core::screen::{
    list_monitors, list_windows, CaptureTarget, ScreenCapture, ScreenOptions, ScreenStatus,
};
use dockview_core::tap_capture::{CaptureSource, TapCapture};

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

#[derive(Default)]
struct Args {
    window: Option<String>,
    monitor: usize,
    audio: Option<String>,
    mic: bool,
    seconds: u64,
    out: Option<PathBuf>,
    dump: Option<PathBuf>,
    mbps: u32,
}

fn parse() -> Result<Args, String> {
    let mut args = Args {
        seconds: 15,
        mbps: 20,
        ..Args::default()
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--window" => args.window = Some(value("--window")?),
            "--monitor" => {
                args.monitor = value("--monitor")?.parse().map_err(|_| "bad --monitor")?
            }
            "--audio" => args.audio = Some(value("--audio")?),
            "--mic" => args.mic = true,
            "--seconds" => {
                args.seconds = value("--seconds")?.parse().map_err(|_| "bad --seconds")?
            }
            "--out" => args.out = Some(PathBuf::from(value("--out")?)),
            "--dump" => args.dump = Some(PathBuf::from(value("--dump")?)),
            "--mbps" => args.mbps = value("--mbps")?.parse().map_err(|_| "bad --mbps")?,
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(args)
}

fn main() {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("screen_record: {e}");
            std::process::exit(2);
        }
    };

    // What to capture, and which program's sound goes with it.
    let (target, pid) = match &args.window {
        Some(text) => {
            let (exact, needle) = match text.strip_prefix('=') {
                Some(t) => (true, t.to_lowercase()),
                None => (false, text.to_lowercase()),
            };
            let windows = list_windows();
            let found = windows.iter().find(|w| {
                let title = w.title.to_lowercase();
                if exact {
                    title == needle
                } else {
                    title.contains(&needle)
                }
            });
            let Some(w) = found else {
                eprintln!("No window matches '{text}'. Windows I can capture:");
                for w in &windows {
                    eprintln!("  {}", w.title);
                }
                std::process::exit(1);
            };
            println!("Window: \"{}\" (process {})", w.title, w.process_id);
            (CaptureTarget::Window(w.handle), Some(w.process_id))
        }
        None => {
            let monitors = list_monitors();
            let Some(m) = monitors.get(args.monitor) else {
                eprintln!("No monitor {} (there are {})", args.monitor, monitors.len());
                std::process::exit(1);
            };
            println!(
                "Monitor {}: {} {}x{}{}",
                args.monitor,
                m.device_name,
                m.width,
                m.height,
                if m.primary { " (primary)" } else { "" }
            );
            (CaptureTarget::Monitor(m.handle), None)
        }
    };

    let slot = Arc::new(FrameSlot::new());
    let capture = ScreenCapture::start(target, ScreenOptions::default(), Arc::clone(&slot));
    let recorder = ReplayRecorder::start(
        Arc::clone(&slot),
        ReplaySettings {
            keep_seconds: 10.0,
            bitrate_mbps: args.mbps,
            fps: 60,
        },
    );

    // Sound.
    let audio_mode = args
        .audio
        .clone()
        .unwrap_or_else(|| if pid.is_some() { "process" } else { "system" }.into());
    let audio_source = match (audio_mode.as_str(), pid) {
        ("none", _) => None,
        ("system", _) => Some(CaptureSource::SystemLoopback),
        ("process", Some(pid)) => Some(CaptureSource::ProcessLoopback(pid)),
        ("process", None) => {
            eprintln!("--audio process needs --window");
            std::process::exit(2);
        }
        (other, _) => {
            eprintln!("unknown audio mode '{other}'");
            std::process::exit(2);
        }
    };
    println!(
        "Audio: {audio_mode}{}",
        if args.mic { " + microphone" } else { "" }
    );
    let _audio = audio_source.map(|s| TapCapture::start(s, 1.0, recorder.audio_tap()));
    let _mic = args
        .mic
        .then(|| TapCapture::start(CaptureSource::Device(None), 1.0, recorder.mic_tap()));

    // Wait for the first frames and the recorder.
    let ready = Instant::now() + Duration::from_secs(10);
    while recorder.status() != RecorderStatus::Recording && Instant::now() < ready {
        thread::sleep(Duration::from_millis(50));
    }
    println!(
        "Capture: {:?}   Recorder: {:?}",
        capture.status(),
        recorder.status()
    );
    if let ScreenStatus::Failed(e) = capture.status() {
        println!("The capture failed: {e}");
        return;
    }

    // Optionally keep one frame, to check the colours.
    if let Some(path) = &args.dump {
        slot.wait_for_frame(Duration::from_secs(2));
        if let Some(frame) = slot.take_latest() {
            let _ = std::fs::write(path, &frame.data);
            println!(
                "Dumped a {}x{} NV12 frame to {}",
                frame.width,
                frame.height,
                path.display()
            );
        }
    }

    let path = args
        .out
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join("dockview-screen-record.mp4"));
    if let Err(e) = recorder.start_recording(path.clone()) {
        println!("Could not start: {e}");
        return;
    }
    let cpu_start = cpu_seconds();
    let started = Instant::now();
    for second in 1..=args.seconds {
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
    let outcome = loop {
        if let Some(o) = recorder.take_recording_result() {
            break o;
        }
        if Instant::now() > deadline {
            println!("The recording never finished.");
            return;
        }
        thread::sleep(Duration::from_millis(20));
    };
    println!("Ended: {:?}", outcome.reason);
    match outcome.result {
        Ok(clip) => println!(
            "\nSaved {}\n  {} frames, {:.2} s ({:.1} fps), {:.1} MB ({:.1} Mbps)\n  CPU of this process while recording: {:.1}% of one core\n  Frames published by the capture: {}",
            clip.path.display(),
            clip.frames,
            clip.seconds,
            clip.frames as f64 / clip.seconds,
            clip.bytes as f64 / 1e6,
            clip.bytes as f64 * 8.0 / clip.seconds / 1e6,
            cpu / elapsed * 100.0,
            slot.published(),
        ),
        Err(e) => println!("Recording failed: {e}"),
    }
}
