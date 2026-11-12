//! Draws two sources into one picture with the scene compositor, records it,
//! and reports what it cost.
//!
//! Run with:
//! `cargo run --release -p dockview-core --example compose_probe -- [options]`
//!
//!   --main <src>      the source drawn over the whole picture (default `monitor:0`)
//!   --pip <src>       a second source in the bottom-right corner (default none)
//!   --pip-size <f>    its width as a fraction of the picture (default 0.28)
//!   --seconds <n>     how long (default 15)
//!   --out <file>      output file (default: a temporary file)
//!   --dump <file>     write one composed picture as raw NV12 for checking
//!   --mbps <n>        video bitrate (default 20)
//!
//! A source is `monitor:<n>`, `window:<title text>` (`window:=Title` for an
//! exact match) or `video:<capture device name text>`.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dockview_core::capture::{CaptureOptions, VideoCapture};
use dockview_core::compose::{Fit, Layer, NormRect, SceneCompositor, SceneStatus};
use dockview_core::frame::FrameSlot;
use dockview_core::mf::{enumerate_video_devices, MfSession};
use dockview_core::recorder::{RecorderStatus, ReplayRecorder, ReplaySettings};
use dockview_core::recording::RecordingState;
use dockview_core::screen::{
    list_monitors, list_windows, CaptureTarget, ScreenCapture, ScreenOptions,
};

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

/// A running source; dropping it stops it.
enum Running {
    Screen(#[allow(dead_code)] ScreenCapture),
    Video(#[allow(dead_code)] VideoCapture),
}

fn start_source(spec: &str) -> Result<(Running, Arc<FrameSlot>), String> {
    let (kind, value) = spec
        .split_once(':')
        .ok_or_else(|| format!("'{spec}': use kind:value, e.g. monitor:0"))?;
    let slot = Arc::new(FrameSlot::new());
    match kind {
        "monitor" => {
            let n: usize = value.parse().map_err(|_| "bad monitor number")?;
            let monitors = list_monitors();
            let m = monitors
                .get(n)
                .ok_or(format!("no monitor {n} (there are {})", monitors.len()))?;
            println!("  monitor {n}: {} {}x{}", m.device_name, m.width, m.height);
            let capture = ScreenCapture::start(
                CaptureTarget::Monitor(m.handle),
                ScreenOptions::default(),
                Arc::clone(&slot),
            );
            Ok((Running::Screen(capture), slot))
        }
        "window" => {
            let (exact, needle) = match value.strip_prefix('=') {
                Some(t) => (true, t.to_lowercase()),
                None => (false, value.to_lowercase()),
            };
            let windows = list_windows();
            let w = windows
                .iter()
                .find(|w| {
                    let title = w.title.to_lowercase();
                    if exact {
                        title == needle
                    } else {
                        title.contains(&needle)
                    }
                })
                .ok_or(format!("no window matches '{value}'"))?;
            println!("  window \"{}\"", w.title);
            let capture = ScreenCapture::start(
                CaptureTarget::Window(w.handle),
                ScreenOptions::default(),
                Arc::clone(&slot),
            );
            Ok((Running::Screen(capture), slot))
        }
        "video" => {
            let devices = {
                let session = MfSession::start().map_err(|e| e.message())?;
                enumerate_video_devices(&session).map_err(|e| e.message())?
            };
            let needle = value.to_lowercase();
            let device = devices
                .iter()
                .find(|d| d.friendly_name.to_lowercase().contains(&needle))
                .ok_or(format!("no capture device matches '{value}'"))?;
            println!("  capture device \"{}\"", device.friendly_name);
            let capture = VideoCapture::start(
                device.symbolic_link.clone(),
                CaptureOptions::default(),
                Arc::clone(&slot),
            );
            Ok((Running::Video(capture), slot))
        }
        other => Err(format!("unknown source kind '{other}'")),
    }
}

fn main() {
    let mut main_spec = "monitor:0".to_string();
    let mut pip_spec: Option<String> = None;
    let mut pip_size = 0.28f32;
    let mut seconds = 15u64;
    let mut out: Option<PathBuf> = None;
    let mut dump: Option<PathBuf> = None;
    let mut mbps = 20u32;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| {
            it.next().unwrap_or_else(|| {
                eprintln!("compose_probe: {name} needs a value");
                std::process::exit(2);
            })
        };
        match arg.as_str() {
            "--main" => main_spec = value("--main"),
            "--pip" => pip_spec = Some(value("--pip")),
            "--pip-size" => pip_size = value("--pip-size").parse().unwrap_or(0.28),
            "--seconds" => seconds = value("--seconds").parse().unwrap_or(15),
            "--out" => out = Some(PathBuf::from(value("--out"))),
            "--dump" => dump = Some(PathBuf::from(value("--dump"))),
            "--mbps" => mbps = value("--mbps").parse().unwrap_or(20),
            other => {
                eprintln!("compose_probe: unknown option '{other}'");
                std::process::exit(2);
            }
        }
    }

    println!("Main source:");
    let (_main_running, main_slot) = start_source(&main_spec).unwrap_or_else(|e| {
        eprintln!("compose_probe: {e}");
        std::process::exit(1);
    });
    let mut layers = vec![Layer::full(Arc::clone(&main_slot))];
    let mut pip_running = None;
    let mut pip_slot = None;
    if let Some(spec) = &pip_spec {
        println!("Second source:");
        let (running, slot) = start_source(spec).unwrap_or_else(|e| {
            eprintln!("compose_probe: {e}");
            std::process::exit(1);
        });
        // Bottom right, 16:9 like the picture, so its height is the same
        // fraction as its width; a small margin from the edges.
        let w = pip_size;
        let h = w;
        let rect = NormRect::new(1.0 - w - 0.02, 1.0 - h - 0.035, w, h);
        layers.push(Layer {
            slot: Arc::clone(&slot),
            rect,
            fit: Fit::Contain,
            visible: true,
            opacity: 1.0,
        });
        pip_running = Some(running);
        pip_slot = Some(slot);
    }

    let composed = Arc::new(FrameSlot::new());
    let scene = SceneCompositor::start(layers, Arc::clone(&composed), 1920, 1080);
    let recorder = ReplayRecorder::start(
        Arc::clone(&composed),
        ReplaySettings {
            keep_seconds: 10.0,
            bitrate_mbps: mbps,
            fps: 60,
        },
    );

    let ready = Instant::now() + Duration::from_secs(10);
    while recorder.status() != RecorderStatus::Recording && Instant::now() < ready {
        thread::sleep(Duration::from_millis(50));
    }
    println!(
        "\nCompositor: {:?}   Recorder: {:?}",
        scene.status(),
        recorder.status()
    );
    if let SceneStatus::Failed(e) = scene.status() {
        println!("The compositor failed: {e}");
        return;
    }

    if let Some(path) = &dump {
        composed.wait_for_frame(Duration::from_secs(2));
        thread::sleep(Duration::from_millis(500));
        if let Some(frame) = composed.take_latest() {
            let _ = std::fs::write(path, &frame.data);
            println!(
                "Dumped a {}x{} NV12 picture to {}",
                frame.width,
                frame.height,
                path.display()
            );
        }
    }

    let path = out.unwrap_or_else(|| std::env::temp_dir().join("dockview-compose-probe.mp4"));
    if let Err(e) = recorder.start_recording(path) {
        println!("Could not start: {e}");
        return;
    }
    let cpu_start = cpu_seconds();
    let started = Instant::now();
    let published_start = composed.published();
    let main_start = main_slot.published();
    let pip_start = pip_slot.as_ref().map_or(0, |s| s.published());
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
    let composed_fps = (composed.published() - published_start) as f64 / elapsed;
    let main_fps = (main_slot.published() - main_start) as f64 / elapsed;
    let pip_fps = pip_slot
        .as_ref()
        .map(|s| (s.published() - pip_start) as f64 / elapsed);
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
        Ok(clip) => {
            println!(
                "\nSaved {}\n  {} frames, {:.2} s ({:.1} fps), {:.1} MB ({:.1} Mbps)",
                clip.path.display(),
                clip.frames,
                clip.seconds,
                clip.frames as f64 / clip.seconds,
                clip.bytes as f64 / 1e6,
                clip.bytes as f64 * 8.0 / clip.seconds / 1e6,
            );
            println!(
                "  CPU of this process (capture + compose + encode): {:.1}% of one core",
                cpu / elapsed * 100.0
            );
            println!("  Main source delivers {main_fps:.1} fps");
            if let Some(fps) = pip_fps {
                println!("  Second source delivers {fps:.1} fps");
            }
            println!("  Composed picture published at {composed_fps:.1} fps");
        }
        Err(e) => println!("Recording failed: {e}"),
    }
    drop(pip_running);
}
