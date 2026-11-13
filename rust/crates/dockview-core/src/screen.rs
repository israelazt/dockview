//! Screen and window capture.
//!
//! Windows Graphics Capture hands over BGRA textures that live on the GPU. The
//! rest of Dockview (viewer, replay, recordings) works on NV12 frames in a
//! [`FrameSlot`], so each captured texture is converted on the GPU with the
//! D3D11 video processor, which also scales it and pads it with black bars to a
//! fixed output size. Only the finished NV12 frame (about 3 MB at 1080p) is
//! copied to RAM, so the CPU hardly notices.
//!
//! The output size is fixed when the capture starts, so a window that is
//! resized or moved while recording does not change the size of the video.

use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use windows::core::{factory, Interface, BOOL};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, D3D11_VIDEO_COLOR_YCbCrA, ID3D11Device, ID3D11DeviceContext,
    ID3D11Multithread, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice,
    ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorOutputView,
    D3D11_BIND_RENDER_TARGET, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_USAGE_STAGING, D3D11_VIDEO_COLOR, D3D11_VIDEO_COLOR_0,
    D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_COLOR_SPACE,
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
    D3D11_VIDEO_USAGE_PLAYBACK_NORMAL, D3D11_VPIV_DIMENSION_TEXTURE2D,
    D3D11_VPOV_DIMENSION_TEXTURE2D,
};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindow, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
};

use crate::frame::{Frame, FrameSlot, PixelFormat};
use crate::worker::{StatusCell, Worker};

/// What to capture. The numbers are the raw `HMONITOR` / `HWND` handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureTarget {
    Monitor(isize),
    Window(isize),
}

/// A monitor that can be captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub handle: isize,
    /// Windows' name for it, e.g. `\\.\DISPLAY1`.
    pub device_name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

/// A window that can be captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub handle: isize,
    pub title: String,
    pub process_id: u32,
}

/// Options for a capture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenOptions {
    /// The video is scaled down to at most this height (never up).
    pub max_height: u32,
    /// The most frames per second to deliver.
    pub max_fps: u32,
    /// Draw the mouse cursor into the video.
    pub cursor: bool,
}

impl Default for ScreenOptions {
    fn default() -> Self {
        Self {
            max_height: 1080,
            max_fps: 60,
            cursor: true,
        }
    }
}

/// State of a screen capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenStatus {
    Starting,
    /// Delivering frames of this (output) size.
    Running {
        width: u32,
        height: u32,
    },
    /// What was being captured went away (the window closed).
    Ended(String),
    Failed(String),
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// The nearest even number not above `n` (NV12 needs even sizes).
pub fn even(n: u32) -> u32 {
    n & !1
}

/// Output size for a source of `width`×`height`: scaled down to at most
/// `max_height` (never up), keeping the shape, and both sides even.
pub fn output_size(width: u32, height: u32, max_height: u32) -> (u32, u32) {
    let (width, height) = (width.max(2), height.max(2));
    if height <= max_height {
        return (even(width).max(2), even(height).max(2));
    }
    let scale = f64::from(max_height) / f64::from(height);
    let w = (f64::from(width) * scale).round() as u32;
    (even(w).max(2), even(max_height).max(2))
}

/// Where a `content_w`×`content_h` picture goes inside an `out_w`×`out_h`
/// canvas: as large as fits, centred, with even coordinates. `(x, y, w, h)`.
pub fn fit_in(content_w: u32, content_h: u32, out_w: u32, out_h: u32) -> (u32, u32, u32, u32) {
    let (content_w, content_h) = (content_w.max(1), content_h.max(1));
    let scale =
        (f64::from(out_w) / f64::from(content_w)).min(f64::from(out_h) / f64::from(content_h));
    let w = even((f64::from(content_w) * scale).round() as u32).clamp(2, out_w);
    let h = even((f64::from(content_h) * scale).round() as u32).clamp(2, out_h);
    (even((out_w - w) / 2), even((out_h - h) / 2), w, h)
}

fn rect(x: u32, y: u32, w: u32, h: u32) -> RECT {
    RECT {
        left: x as i32,
        top: y as i32,
        right: (x + w) as i32,
        bottom: (y + h) as i32,
    }
}

// ---------------------------------------------------------------------------
// Listing what can be captured
// ---------------------------------------------------------------------------

unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<MonitorInfo>` passed to EnumDisplayMonitors.
    let list = unsafe { &mut *(data.0 as *mut Vec<MonitorInfo>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `info` is a valid MONITORINFOEXW with cbSize set.
    if unsafe {
        GetMonitorInfoW(
            monitor,
            (&mut info as *mut MONITORINFOEXW).cast::<MONITORINFO>(),
        )
    }
    .as_bool()
    {
        let r = info.monitorInfo.rcMonitor;
        let name_len = info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szDevice.len());
        list.push(MonitorInfo {
            handle: monitor.0 as isize,
            device_name: String::from_utf16_lossy(&info.szDevice[..name_len]),
            width: (r.right - r.left).max(0) as u32,
            height: (r.bottom - r.top).max(0) as u32,
            primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
        });
    }
    BOOL(1)
}

/// The monitors, the primary one first.
pub fn list_monitors() -> Vec<MonitorInfo> {
    let mut list: Vec<MonitorInfo> = Vec::new();
    // SAFETY: the callback only touches `list`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_callback),
            LPARAM(&mut list as *mut Vec<MonitorInfo> as isize),
        );
    }
    list.sort_by_key(|m| !m.primary);
    list
}

unsafe extern "system" fn window_callback(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<WindowInfo>` passed to EnumWindows.
    let list = unsafe { &mut *(data.0 as *mut Vec<WindowInfo>) };
    // SAFETY: plain queries on a window handle from EnumWindows.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return BOOL(1);
        }
        // Owned windows (dialogs) and tool windows are not "applications".
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.0.is_null()) {
            return BOOL(1);
        }
        if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }
        // Windows on other virtual desktops and suspended apps are "cloaked".
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast::<c_void>(),
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return BOOL(1);
        }
        let mut buffer = [0u16; 512];
        let length = GetWindowTextW(hwnd, &mut buffer) as usize;
        let title = String::from_utf16_lossy(&buffer[..length]);
        if title.trim().is_empty() {
            return BOOL(1);
        }
        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        list.push(WindowInfo {
            handle: hwnd.0 as isize,
            title,
            process_id,
        });
    }
    BOOL(1)
}

/// The application windows that can be captured, in the order Windows lists
/// them (front to back).
pub fn list_windows() -> Vec<WindowInfo> {
    let mut list: Vec<WindowInfo> = Vec::new();
    // SAFETY: the callback only touches `list`, which outlives the call.
    unsafe {
        let _ = EnumWindows(
            Some(window_callback),
            LPARAM(&mut list as *mut Vec<WindowInfo> as isize),
        );
    }
    list
}

/// Whether a window handle still refers to a window.
pub fn window_exists(handle: isize) -> bool {
    // SAFETY: IsWindow accepts any handle value.
    unsafe { IsWindow(Some(HWND(handle as *mut c_void))).as_bool() }
}

// ---------------------------------------------------------------------------
// GPU conversion
// ---------------------------------------------------------------------------

/// Converts captured BGRA textures to NV12 on the GPU and reads them back.
struct Converter {
    context: ID3D11DeviceContext,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    output_view: ID3D11VideoProcessorOutputView,
    output: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    out_w: u32,
    out_h: u32,
    /// The picture size the destination rectangle was last computed for.
    laid_out_for: Option<(u32, u32)>,
}

fn nv12_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    staging: bool,
) -> windows::core::Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: if staging {
            D3D11_USAGE_STAGING
        } else {
            D3D11_USAGE_DEFAULT
        },
        BindFlags: if staging {
            0
        } else {
            D3D11_BIND_RENDER_TARGET.0 as u32
        },
        CPUAccessFlags: if staging {
            D3D11_CPU_ACCESS_READ.0 as u32
        } else {
            0
        },
        MiscFlags: 0,
    };
    let mut texture: Option<ID3D11Texture2D> = None;
    // SAFETY: valid description and out-pointer.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture))? };
    texture.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))
}

impl Converter {
    fn new(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        in_size: (u32, u32),
        out_w: u32,
        out_h: u32,
    ) -> windows::core::Result<Self> {
        let video_device: ID3D11VideoDevice = device.cast()?;
        let video_context: ID3D11VideoContext = context.cast()?;
        let rate = DXGI_RATIONAL {
            Numerator: 60,
            Denominator: 1,
        };
        let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: rate,
            InputWidth: in_size.0,
            InputHeight: in_size.1,
            OutputFrameRate: rate,
            OutputWidth: out_w,
            OutputHeight: out_h,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };
        // SAFETY: documented video-processor setup with valid descriptions.
        unsafe {
            let enumerator = video_device.CreateVideoProcessorEnumerator(&content)?;
            let processor = video_device.CreateVideoProcessor(&enumerator, 0)?;

            let output = nv12_texture(device, out_w, out_h, false)?;
            let staging = nv12_texture(device, out_w, out_h, true)?;
            let view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut output_view: Option<ID3D11VideoProcessorOutputView> = None;
            video_device.CreateVideoProcessorOutputView(
                &output,
                &enumerator,
                &view_desc,
                Some(&mut output_view),
            )?;
            let output_view = output_view
                .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;

            // RGB in (full range) -> BT.709 video in (limited range, 16..235),
            // the same colours the capture card's NV12 uses.
            let input_space = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
                _bitfield: 2 << 4, // RGB_Range 0 (full), Nominal_Range 0..255
            };
            let output_space = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
                _bitfield: (1 << 2) | (1 << 4), // YCbCr_Matrix 709, Nominal_Range 16..235
            };
            video_context.VideoProcessorSetStreamColorSpace(&processor, 0, &input_space);
            video_context.VideoProcessorSetOutputColorSpace(&processor, &output_space);
            let full = rect(0, 0, out_w, out_h);
            video_context.VideoProcessorSetOutputTargetRect(&processor, true, Some(&full));
            // Black bars, in video levels.
            let black = D3D11_VIDEO_COLOR {
                Anonymous: D3D11_VIDEO_COLOR_0 {
                    YCbCr: D3D11_VIDEO_COLOR_YCbCrA {
                        Y: 16.0 / 255.0,
                        Cb: 0.5,
                        Cr: 0.5,
                        A: 1.0,
                    },
                },
            };
            video_context.VideoProcessorSetOutputBackgroundColor(&processor, true, &black);
            video_context.VideoProcessorSetStreamFrameFormat(
                &processor,
                0,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            );

            Ok(Self {
                context: context.clone(),
                video_device,
                video_context,
                enumerator,
                processor,
                output_view,
                output,
                staging,
                out_w,
                out_h,
                laid_out_for: None,
            })
        }
    }

    /// Converts `texture` (BGRA; only its top-left `content` part is the
    /// picture) and publishes the NV12 result to `slot`.
    fn convert(
        &mut self,
        texture: &ID3D11Texture2D,
        content: (u32, u32),
        slot: &FrameSlot,
        timestamp: i64,
    ) -> windows::core::Result<()> {
        // SAFETY: video-processor and copy calls on resources of this device,
        // used from one thread at a time (the caller holds a lock).
        unsafe {
            if self.laid_out_for != Some(content) {
                let source = rect(0, 0, content.0, content.1);
                let (x, y, w, h) = fit_in(content.0, content.1, self.out_w, self.out_h);
                let dest = rect(x, y, w, h);
                self.video_context.VideoProcessorSetStreamSourceRect(
                    &self.processor,
                    0,
                    true,
                    Some(&source),
                );
                self.video_context.VideoProcessorSetStreamDestRect(
                    &self.processor,
                    0,
                    true,
                    Some(&dest),
                );
                self.laid_out_for = Some(content);
            }

            let view_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV {
                        MipSlice: 0,
                        ArraySlice: 0,
                    },
                },
            };
            let mut input_view = None;
            self.video_device.CreateVideoProcessorInputView(
                texture,
                &self.enumerator,
                &view_desc,
                Some(&mut input_view),
            )?;
            let stream = D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: BOOL(1),
                pInputSurface: ManuallyDrop::new(input_view),
                ..Default::default()
            };
            let result = self.video_context.VideoProcessorBlt(
                &self.processor,
                &self.output_view,
                0,
                std::slice::from_ref(&stream),
            );
            // The stream held the only extra reference to the view.
            drop(ManuallyDrop::into_inner(stream.pInputSurface));
            result?;

            self.context.CopyResource(&self.staging, &self.output);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let (w, h) = (self.out_w as usize, self.out_h as usize);
            let expected = PixelFormat::Nv12.buffer_len(w, h);
            let mut frame = slot
                .take_spare()
                .filter(|f| {
                    f.format == PixelFormat::Nv12
                        && f.width == self.out_w
                        && f.height == self.out_h
                        && f.data.len() == expected
                })
                .unwrap_or_else(|| Frame {
                    width: self.out_w,
                    height: self.out_h,
                    stride: self.out_w,
                    format: PixelFormat::Nv12,
                    data: vec![0; expected],
                    timestamp: 0,
                    arrived: Instant::now(),
                });
            // The luma rows, then the chroma rows right after them.
            let pitch = mapped.RowPitch as usize;
            let base = mapped.pData as *const u8;
            for row in 0..h + h / 2 {
                let line = std::slice::from_raw_parts(base.add(row * pitch), w);
                frame.data[row * w..(row + 1) * w].copy_from_slice(line);
            }
            self.context.Unmap(&self.staging, 0);
            frame.timestamp = timestamp;
            slot.publish(frame);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The capture
// ---------------------------------------------------------------------------

/// What the frame callback (which runs on a thread of Windows' pool) uses.
struct Gpu {
    converter: Mutex<Converter>,
    winrt_device: IDirect3DDevice,
}

// SAFETY: the D3D11 device is created with multithread protection, every use of
// the converter happens under its mutex, and the WinRT device wrapper is only
// handed to the capture pool, which is free-threaded.
unsafe impl Send for Gpu {}
unsafe impl Sync for Gpu {}

/// Captures a monitor or a window into a [`FrameSlot`] while it is alive.
pub struct ScreenCapture {
    status: Arc<StatusCell<ScreenStatus>>,
    _worker: Worker,
}

/// How long dropping a capture waits for its thread.
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

impl ScreenCapture {
    pub fn start(target: CaptureTarget, options: ScreenOptions, slot: Arc<FrameSlot>) -> Self {
        let status = Arc::new(StatusCell::new(ScreenStatus::Starting));
        let mut worker = Worker::new(STOP_TIMEOUT);
        let thread_status = Arc::clone(&status);
        let started = worker.spawn("screen-capture", move |stop| {
            if let Err(e) = run(target, options, &slot, stop, &thread_status) {
                thread_status.set(ScreenStatus::Failed(e.message()));
            }
        });
        if !started {
            status.set(ScreenStatus::Failed(
                "could not start the capture thread".into(),
            ));
        }
        Self {
            status,
            _worker: worker,
        }
    }

    pub fn status(&self) -> ScreenStatus {
        self.status.get()
    }
}

fn run(
    target: CaptureTarget,
    options: ScreenOptions,
    slot: &Arc<FrameSlot>,
    stop: &AtomicBool,
    status: &Arc<StatusCell<ScreenStatus>>,
) -> windows::core::Result<()> {
    // SAFETY: first WinRT initialisation on this thread (already-initialised
    // is fine and ignored).
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }

    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    // SAFETY: handles come from the listing functions; a stale one is an error.
    let item: GraphicsCaptureItem = unsafe {
        match target {
            CaptureTarget::Monitor(h) => interop.CreateForMonitor(HMONITOR(h as *mut c_void))?,
            CaptureTarget::Window(h) => interop.CreateForWindow(HWND(h as *mut c_void))?,
        }
    };
    let size = item.Size()?;
    let (out_w, out_h) = output_size(
        size.Width.max(2) as u32,
        size.Height.max(2) as u32,
        options.max_height,
    );

    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    // SAFETY: valid out-pointers, default adapter.
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    let (Some(device), Some(context)) = (device, context) else {
        return Err(windows::Win32::Foundation::E_FAIL.into());
    };
    // The capture delivers frames on its own threads.
    let multithread: ID3D11Multithread = context.cast()?;
    // SAFETY: plain setter.
    unsafe {
        let _ = multithread.SetMultithreadProtected(true);
    }
    let dxgi: IDXGIDevice = device.cast()?;
    // SAFETY: wraps the DXGI device for WinRT.
    let winrt_device: IDirect3DDevice =
        unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;

    let gpu = Arc::new(Gpu {
        converter: Mutex::new(Converter::new(
            &device,
            &context,
            (size.Width.max(2) as u32, size.Height.max(2) as u32),
            out_w,
            out_h,
        )?),
        winrt_device,
    });

    const FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&gpu.winrt_device, FORMAT, 3, size)?;
    let pool_size = Arc::new(Mutex::new(size));

    let handler = {
        let (gpu, slot, status, pool_size) = (
            Arc::clone(&gpu),
            Arc::clone(slot),
            Arc::clone(status),
            Arc::clone(&pool_size),
        );
        TypedEventHandler::<Direct3D11CaptureFramePool, windows::core::IInspectable>::new(
            move |pool, _| {
                let Some(pool) = pool.as_ref() else {
                    return Ok(());
                };
                while let Ok(frame) = pool.TryGetNextFrame() {
                    let content = frame.ContentSize()?;
                    {
                        // The window changed size: follow it with the buffers,
                        // while the output keeps its size (the picture is
                        // rescaled into it).
                        let mut current = pool_size.lock().unwrap_or_else(|e| e.into_inner());
                        if (current.Width, current.Height) != (content.Width, content.Height) {
                            pool.Recreate(&gpu.winrt_device, FORMAT, 3, content)?;
                            *current = content;
                        }
                    }
                    let surface = frame.Surface()?;
                    let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
                    // SAFETY: the surface is a D3D11 texture on our device.
                    let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
                    let timestamp = frame.SystemRelativeTime()?.Duration;
                    let mut converter = gpu.converter.lock().unwrap_or_else(|e| e.into_inner());
                    match converter.convert(
                        &texture,
                        (content.Width.max(1) as u32, content.Height.max(1) as u32),
                        &slot,
                        timestamp,
                    ) {
                        Ok(()) => status.set(ScreenStatus::Running {
                            width: converter.out_w,
                            height: converter.out_h,
                        }),
                        Err(e) => status.set(ScreenStatus::Failed(e.message())),
                    }
                }
                Ok(())
            },
        )
    };
    pool.FrameArrived(&handler)?;

    let closed = {
        let status = Arc::clone(status);
        TypedEventHandler::<GraphicsCaptureItem, windows::core::IInspectable>::new(move |_, _| {
            status.set(ScreenStatus::Ended("the captured window closed".into()));
            Ok(())
        })
    };
    item.Closed(&closed)?;

    let session = pool.CreateCaptureSession(&item)?;
    let _ = session.SetIsCursorCaptureEnabled(options.cursor);
    // Not allowed everywhere (older Windows, or policy): the border is harmless.
    let _ = session.SetIsBorderRequired(false);
    // Windows skips frames that come closer together than this; the default
    // loses about a fifth of them at 60 fps. Aim just above the wanted rate.
    let interval_ms = 1000.0 / f64::from(options.max_fps.max(1) + 10);
    let _ = session.SetMinUpdateInterval(TimeSpan {
        Duration: (interval_ms * 10_000.0) as i64,
    });
    session.StartCapture()?;

    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
        if let CaptureTarget::Window(handle) = target {
            if !window_exists(handle) {
                status.set(ScreenStatus::Ended("the captured window closed".into()));
            }
        }
    }
    session.Close()?;
    pool.Close()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_made_even() {
        assert_eq!(even(1920), 1920);
        assert_eq!(even(1081), 1080);
        assert_eq!(even(3), 2);
    }

    #[test]
    fn output_size_scales_down_but_never_up_and_stays_even() {
        assert_eq!(output_size(1920, 1080, 1080), (1920, 1080));
        assert_eq!(output_size(1280, 720, 1080), (1280, 720), "never scaled up");
        assert_eq!(output_size(2560, 1440, 1080), (1920, 1080));
        assert_eq!(output_size(3840, 2160, 1080), (1920, 1080));
        // Odd window sizes are rounded down to even.
        assert_eq!(output_size(801, 601, 1080), (800, 600));
        // A tall window is limited by its height.
        let (w, h) = output_size(1000, 2000, 1080);
        assert_eq!(h, 1080);
        assert!(w % 2 == 0 && (w as f64 - 540.0).abs() <= 2.0, "{w}");
        // Degenerate input still gives a usable size.
        assert_eq!(output_size(0, 0, 1080), (2, 2));
    }

    #[test]
    fn a_picture_is_fitted_and_centred_with_black_bars() {
        // Same shape: fills the canvas.
        assert_eq!(fit_in(1920, 1080, 1920, 1080), (0, 0, 1920, 1080));
        // A 4:3 picture in a 16:9 canvas: bars left and right.
        let (x, y, w, h) = fit_in(1600, 1200, 1920, 1080);
        assert_eq!((y, h), (0, 1080));
        assert_eq!(w, 1440);
        assert_eq!(x, 240);
        // A window that grew: rescaled into the same canvas.
        let (x, y, w, h) = fit_in(3000, 1000, 1920, 1080);
        assert_eq!((x, w), (0, 1920));
        assert!(y > 0 && h < 1080 && h % 2 == 0 && y % 2 == 0);
        // Everything stays inside the canvas and even.
        for (cw, ch) in [(1, 1), (7, 3), (5000, 100), (100, 5000)] {
            let (x, y, w, h) = fit_in(cw, ch, 1280, 720);
            assert!(x + w <= 1280 && y + h <= 720, "{cw}x{ch}");
            assert!(
                x % 2 == 0 && y % 2 == 0 && w % 2 == 0 && h % 2 == 0,
                "{cw}x{ch}"
            );
        }
    }

    #[test]
    fn monitors_are_listed_with_the_primary_first() {
        let monitors = list_monitors();
        assert!(!monitors.is_empty(), "this PC has at least one monitor");
        assert!(monitors[0].primary);
        assert!(monitors.iter().all(|m| m.width > 0 && m.height > 0));
    }

    #[test]
    fn listing_windows_does_not_crash_and_skips_untitled_ones() {
        let windows = list_windows();
        assert!(windows
            .iter()
            .all(|w| !w.title.trim().is_empty() && w.process_id != 0));
    }

    #[test]
    fn a_made_up_window_handle_does_not_exist() {
        assert!(!window_exists(0x7FFF_FFF0));
    }

    #[test]
    fn capturing_the_primary_monitor_delivers_even_sized_nv12_frames() {
        let Some(monitor) = list_monitors().into_iter().next() else {
            return;
        };
        let slot = Arc::new(FrameSlot::new());
        let capture = ScreenCapture::start(
            CaptureTarget::Monitor(monitor.handle),
            ScreenOptions::default(),
            Arc::clone(&slot),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut frame = None;
        while Instant::now() < deadline && frame.is_none() {
            if slot.wait_for_frame(Duration::from_millis(100)) {
                frame = slot.take_latest();
            }
        }
        let status = capture.status();
        let Some(frame) = frame else {
            // No desktop session (for example a service): nothing to check.
            eprintln!("skipped: no frames, status {status:?}");
            return;
        };
        assert_eq!(frame.format, PixelFormat::Nv12);
        assert!(frame.width % 2 == 0 && frame.height % 2 == 0);
        assert!(frame.height <= 1080);
        assert_eq!(
            frame.data.len(),
            (frame.width * frame.height * 3 / 2) as usize
        );
        assert_eq!(frame.stride, frame.width);
        assert!(matches!(status, ScreenStatus::Running { .. }), "{status:?}");
    }
}
