//! Spike: capture a monitor or a window (Windows Graphics Capture) and the
//! system's or one program's audio (WASAPI loopback), and report what it costs.
//!
//! No interface and no encoding: this only answers "can Dockview capture a PC
//! game, and how much does that take away from the game?".
//!
//! Run with:
//! `cargo run --release -p dockview-core --example screen_probe -- [options]`
//!
//! Options:
//!   --window <text>   capture the first visible window whose title contains
//!                     <text> (e.g. `League`; `=Title` must match exactly);
//!                     default is the primary monitor
//!   --audio <mode>    `none` (default), `system`, or `process` (only the
//!                     program that owns the window; needs --window)
//!   --readback        also copy every frame to the CPU (the pessimistic case)
//!   --seconds <n>     how long to run (default 15)
//!   --min-interval-ms <ms>  ask Windows 11 for a smaller minimum gap
//!                     between delivered frames (default: Windows' own)

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use wasapi::{
    initialize_mta, AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat,
};
use windows::core::{factory, Interface, BOOL};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{FILETIME, HMODULE, HWND, LPARAM, POINT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{MonitorFromPoint, HMONITOR, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

#[derive(Clone, Copy, PartialEq)]
enum AudioMode {
    None,
    System,
    Process,
}

struct Options {
    window: Option<String>,
    audio: AudioMode,
    readback: bool,
    seconds: u64,
    min_interval_ms: Option<f64>,
}

fn parse_args() -> Result<Options, String> {
    let mut options = Options {
        window: None,
        audio: AudioMode::None,
        readback: false,
        seconds: 15,
        min_interval_ms: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--window" => options.window = Some(value("--window")?),
            "--audio" => {
                options.audio = match value("--audio")?.as_str() {
                    "none" => AudioMode::None,
                    "system" => AudioMode::System,
                    "process" => AudioMode::Process,
                    other => return Err(format!("unknown audio mode '{other}'")),
                }
            }
            "--readback" => options.readback = true,
            "--min-interval-ms" => {
                options.min_interval_ms = Some(
                    value("--min-interval-ms")?
                        .parse()
                        .map_err(|_| "--min-interval-ms needs a number".to_string())?,
                )
            }
            "--seconds" => {
                options.seconds = value("--seconds")?
                    .parse()
                    .map_err(|_| "--seconds needs a number".to_string())?
            }
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    if options.audio == AudioMode::Process && options.window.is_none() {
        return Err("--audio process needs --window".into());
    }
    Ok(options)
}

// ---------------------------------------------------------------------------
// Finding a window by title
// ---------------------------------------------------------------------------

struct Found {
    needle: String,
    hwnd: Option<HWND>,
    title: String,
    pid: u32,
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Found` passed to EnumWindows below.
    let found = unsafe { &mut *(lparam.0 as *mut Found) };
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return BOOL(1);
    }
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) } as usize;
    let title = String::from_utf16_lossy(&buffer[..length]);
    let lower = title.to_lowercase();
    let matches = match found.needle.strip_prefix('=') {
        Some(exact) => lower == exact,
        None => lower.contains(&found.needle),
    };
    if !title.is_empty() && matches {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        found.hwnd = Some(hwnd);
        found.title = title;
        found.pid = pid;
        return BOOL(0); // stop at the first match
    }
    BOOL(1)
}

fn find_window(text: &str) -> Option<(HWND, String, u32)> {
    let mut found = Found {
        needle: text.to_lowercase(),
        hwnd: None,
        title: String::new(),
        pid: 0,
    };
    // SAFETY: `found` outlives the synchronous EnumWindows call.
    unsafe {
        let _ = EnumWindows(Some(enum_window), LPARAM(&mut found as *mut Found as isize));
    }
    found.hwnd.map(|h| (h, found.title, found.pid))
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

#[derive(Default)]
struct VideoStats {
    frames: AtomicU64,
    last: Mutex<Option<Instant>>,
    first: Mutex<Option<Instant>>,
    intervals_ms: Mutex<Vec<f64>>,
}

impl VideoStats {
    fn record(&self) {
        let now = Instant::now();
        self.frames.fetch_add(1, Ordering::Relaxed);
        self.first
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert(now);
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(previous) = last.replace(now) {
            self.intervals_ms
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(now.duration_since(previous).as_secs_f64() * 1000.0);
        }
    }
}

#[derive(Default)]
struct AudioStats {
    blocks: AtomicU64,
    frames: AtomicU64,
    /// Sum of squares ×1e6 (integer so it can be atomic) — see `rms`.
    first: Mutex<Option<Instant>>,
    sum_squares: Mutex<f64>,
    peak: Mutex<f32>,
    error: Mutex<Option<String>>,
}

fn process_cpu_seconds() -> f64 {
    let (mut creation, mut exit, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: valid out-pointers for the current process.
    unsafe {
        let _ = GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        );
    }
    let ticks =
        |f: FILETIME| ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as f64;
    (ticks(kernel) + ticks(user)) / 1e7
}

// ---------------------------------------------------------------------------
// Audio loopback
// ---------------------------------------------------------------------------

fn audio_loop(
    mode: AudioMode,
    pid: u32,
    stop: &AtomicBool,
    stats: &AudioStats,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = initialize_mta();
    let format = WaveFormat::new(32, 32, SampleType::Float, 48_000, 2, None);
    let block_align = format.get_blockalign() as usize;
    let mut client = match mode {
        AudioMode::Process => AudioClient::new_application_loopback_client(pid, true)?,
        _ => {
            let enumerator = DeviceEnumerator::new()?;
            let device = enumerator.get_default_device(Direction::Render)?;
            device.get_iaudioclient()?
        }
    };
    let stream_mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: if mode == AudioMode::Process {
            0
        } else {
            200_000
        },
    };
    client.initialize_client(&format, Direction::Capture, stream_mode, None)?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;

    let mut bytes: Vec<u8> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let _ = event.wait_for_event(100);
        while let Some(frames) = capture.get_next_packet_size()? {
            if frames == 0 {
                break;
            }
            bytes.resize(frames as usize * block_align, 0);
            let (read, _) = capture.read_from_device(&mut bytes)?;
            let used = read as usize * block_align;
            stats
                .first
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(Instant::now);
            stats.blocks.fetch_add(1, Ordering::Relaxed);
            stats.frames.fetch_add(u64::from(read), Ordering::Relaxed);
            let (words, _) = bytes[..used].as_chunks::<4>();
            let (mut squares, mut peak) = (0.0f64, 0.0f32);
            for word in words {
                let v = f32::from_le_bytes(*word);
                squares += f64::from(v) * f64::from(v);
                peak = peak.max(v.abs());
            }
            *stats.sum_squares.lock().unwrap_or_else(|e| e.into_inner()) += squares;
            let mut p = stats.peak.lock().unwrap_or_else(|e| e.into_inner());
            *p = p.max(peak);
        }
    }
    let _ = client.stop_stream();
    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let options = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("screen_probe: {e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = run(&options) {
        eprintln!("screen_probe failed: {e}");
        std::process::exit(1);
    }
}

fn run(options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    // SAFETY: first COM/WinRT initialisation on this thread.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };

    // What to capture.
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    let mut pid = 0u32;
    let item: GraphicsCaptureItem = match &options.window {
        Some(text) => {
            let (hwnd, title, owner) =
                find_window(text).ok_or(format!("no visible window matches '{text}'"))?;
            println!("Window: \"{title}\" (process {owner})");
            pid = owner;
            // SAFETY: `hwnd` is a live window handle from EnumWindows.
            unsafe { interop.CreateForWindow(hwnd)? }
        }
        None => {
            // SAFETY: plain query for the primary monitor.
            let monitor: HMONITOR =
                unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
            println!("Primary monitor");
            unsafe { interop.CreateForMonitor(monitor)? }
        }
    };
    let size = item.Size()?;
    println!("Capture size {}x{}", size.Width, size.Height);

    // A D3D11 device the capture can deliver textures on.
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    // SAFETY: valid out-pointers, default adapter.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    let device = device.ok_or("no D3D11 device")?;
    let context = context.ok_or("no D3D11 context")?;
    let dxgi: IDXGIDevice = device.cast()?;
    // SAFETY: wraps the DXGI device for WinRT.
    let winrt_device: IDirect3DDevice =
        unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;

    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &winrt_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        3,
        size,
    )?;
    let session = pool.CreateCaptureSession(&item)?;
    let _ = session.SetIsCursorCaptureEnabled(true);
    // Not allowed everywhere (older Windows, or policy): the border is harmless.
    let _ = session.SetIsBorderRequired(false);
    // Windows 11 22H2+: by default frames closer than ~16 ms apart can be
    // skipped. Ask for every change (1 ms minimum interval).
    let min_interval = options.min_interval_ms;
    if let Some(ms) = min_interval {
        match session.SetMinUpdateInterval(windows::Foundation::TimeSpan {
            Duration: (ms * 10_000.0) as i64,
        }) {
            Ok(()) => println!("Minimum update interval set to {ms} ms"),
            Err(e) => println!("Could not set the minimum update interval: {e}"),
        }
    }

    let video = Arc::new(VideoStats::default());
    let readback = options.readback;
    // Staging texture for the CPU copy, created on the first frame.
    let staging: Arc<Mutex<Option<ID3D11Texture2D>>> = Arc::new(Mutex::new(None));
    let readback_bytes = Arc::new(AtomicU64::new(0));

    let handler = {
        let (video, staging, readback_bytes) = (
            Arc::clone(&video),
            Arc::clone(&staging),
            Arc::clone(&readback_bytes),
        );
        let (device, context) = (device.clone(), context.clone());
        TypedEventHandler::<Direct3D11CaptureFramePool, windows::core::IInspectable>::new(
            move |pool, _| {
                let Some(pool) = pool.as_ref() else {
                    return Ok(());
                };
                while let Ok(frame) = pool.TryGetNextFrame() {
                    video.record();
                    if readback {
                        let surface = frame.Surface()?;
                        let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
                        // SAFETY: the surface is a D3D11 texture on our device.
                        let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
                        let mut staging = staging.lock().unwrap_or_else(|e| e.into_inner());
                        if staging.is_none() {
                            let mut desc = D3D11_TEXTURE2D_DESC::default();
                            unsafe { texture.GetDesc(&mut desc) };
                            desc.Usage = D3D11_USAGE_STAGING;
                            desc.BindFlags = 0;
                            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                            desc.MiscFlags = 0;
                            let mut created: Option<ID3D11Texture2D> = None;
                            unsafe { device.CreateTexture2D(&desc, None, Some(&mut created))? };
                            *staging = created;
                        }
                        if let Some(staging) = staging.as_ref() {
                            // SAFETY: copy then map the staging texture on the
                            // device's immediate context (used only here).
                            unsafe {
                                context.CopyResource(staging, &texture);
                                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                                context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                                // Touch the first and last row so the copy is real.
                                let mut desc = D3D11_TEXTURE2D_DESC::default();
                                staging.GetDesc(&mut desc);
                                let last = (desc.Height as usize - 1) * mapped.RowPitch as usize;
                                let base = mapped.pData as *const u8;
                                let sum = u32::from(*base) + u32::from(*base.add(last));
                                readback_bytes.fetch_add(
                                    u64::from(mapped.RowPitch) * u64::from(desc.Height)
                                        + u64::from(sum & 1),
                                    Ordering::Relaxed,
                                );
                                context.Unmap(staging, 0);
                            }
                        }
                    }
                }
                Ok(())
            },
        )
    };
    pool.FrameArrived(&handler)?;

    // Audio.
    let stop = Arc::new(AtomicBool::new(false));
    let audio = Arc::new(AudioStats::default());
    let audio_thread = (options.audio != AudioMode::None).then(|| {
        let (stop, audio, mode) = (Arc::clone(&stop), Arc::clone(&audio), options.audio);
        thread::spawn(move || {
            if let Err(e) = audio_loop(mode, pid, &stop, &audio) {
                *audio.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
            }
        })
    });

    let started = Instant::now();
    session.StartCapture()?;
    println!(
        "Capturing for {} s{}…",
        options.seconds,
        if options.readback {
            " (with CPU readback)"
        } else {
            ""
        }
    );

    let cpu_start = process_cpu_seconds();
    let (mut last_frames, mut last_cpu) = (0u64, cpu_start);
    for second in 1..=options.seconds {
        thread::sleep(Duration::from_secs(1));
        let frames = video.frames.load(Ordering::Relaxed);
        let cpu = process_cpu_seconds();
        println!(
            "t={second:>2}s  video {:>3} fps  cpu {:>4.1}%  audio blocks {:>4}",
            frames - last_frames,
            (cpu - last_cpu) * 100.0,
            audio.blocks.load(Ordering::Relaxed),
        );
        (last_frames, last_cpu) = (frames, cpu);
    }
    let elapsed = started.elapsed().as_secs_f64();
    let cpu_total = process_cpu_seconds() - cpu_start;

    session.Close()?;
    pool.Close()?;
    stop.store(true, Ordering::Relaxed);
    if let Some(t) = audio_thread {
        let _ = t.join();
    }

    // Summary.
    let frames = video.frames.load(Ordering::Relaxed);
    let mut intervals = video
        .intervals_ms
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    intervals.sort_by(f64::total_cmp);
    let pick = |q: f64| {
        intervals
            .get(((intervals.len() as f64 - 1.0) * q) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    println!("\n=== Summary ===");
    println!(
        "Video: {frames} frames in {elapsed:.1} s = {:.1} fps",
        frames as f64 / elapsed
    );
    println!(
        "Frame interval ms: median {:.1}, p99 {:.1}, max {:.1}",
        pick(0.5),
        pick(0.99),
        intervals.last().copied().unwrap_or(0.0)
    );
    println!(
        "CPU of this process: {:.1}% of one core{}",
        cpu_total / elapsed * 100.0,
        if options.readback {
            " (readback on)"
        } else {
            ""
        }
    );
    if options.audio != AudioMode::None {
        if let Some(e) = audio
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            println!("Audio FAILED: {e}");
        } else {
            let frames = audio.frames.load(Ordering::Relaxed);
            let rms = (*audio.sum_squares.lock().unwrap_or_else(|e| e.into_inner())
                / (frames.max(1) as f64 * 2.0))
                .sqrt();
            let first_video = *video.first.lock().unwrap_or_else(|e| e.into_inner());
            let first_audio = *audio.first.lock().unwrap_or_else(|e| e.into_inner());
            let lead = match (first_video, first_audio) {
                (Some(v), Some(a)) => format!(
                    "{:+.0} ms (audio minus video)",
                    (a.duration_since(v.min(a)).as_secs_f64()
                        - v.duration_since(v.min(a)).as_secs_f64())
                        * 1000.0
                ),
                _ => "n/a".into(),
            };
            println!(
                "Audio: {} blocks, {:.1} s of sound, RMS {:.4}, peak {:.3}, first block vs first frame {lead}",
                audio.blocks.load(Ordering::Relaxed),
                frames as f64 / 48_000.0,
                rms,
                *audio.peak.lock().unwrap_or_else(|e| e.into_inner()),
            );
            if rms < 0.0005 {
                println!("  (silent: nothing was playing, or the wrong program was captured)");
            }
        }
    }
    Ok(())
}
