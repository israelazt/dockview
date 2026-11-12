//! Draws several video sources into one picture on the GPU: the scene
//! compositor.
//!
//! Every source keeps publishing to its own [`FrameSlot`]. A [`Compositor`]
//! uploads the newest frame of each one, lets Direct3D 11's video processor
//! draw them, bottom layer first, into one NV12 texture (it also converts,
//! scales and crops), and publishes the result to an output slot. The viewer,
//! the replay and the recorder read that output exactly as they read a single
//! source today.
//!
//! No shaders are needed: the video processor composites up to
//! [`Compositor::max_layers`] streams in one call, straight to NV12.

use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use windows::core::{Interface, BOOL};
use windows::Win32::Foundation::{E_FAIL, HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, D3D11_VIDEO_COLOR_YCbCrA, ID3D11Device, ID3D11DeviceContext,
    ID3D11Multithread, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice,
    ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView,
    ID3D11VideoProcessorOutputView, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
    D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEX2D_VPIV,
    D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
    D3D11_VIDEO_COLOR, D3D11_VIDEO_COLOR_0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
    D3D11_VIDEO_PROCESSOR_CAPS, D3D11_VIDEO_PROCESSOR_COLOR_SPACE,
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
    D3D11_VIDEO_USAGE_PLAYBACK_NORMAL, D3D11_VPIV_DIMENSION_TEXTURE2D,
    D3D11_VPOV_DIMENSION_TEXTURE2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_NV12, DXGI_FORMAT_YUY2, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};

use crate::frame::{Frame, FrameSlot, PixelFormat};
use crate::screen::fit_in;

/// A rectangle as fractions of the output picture: `0.0, 0.0, 1.0, 1.0` is all
/// of it. Free-form, so a scene editor can place layers anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl NormRect {
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

/// How a source's picture fills its rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Fit {
    /// The whole picture, kept in proportion, with black bars if needed.
    #[default]
    Contain,
    /// The rectangle is filled in proportion, cropping what sticks out.
    Cover,
    /// The whole picture stretched to the rectangle.
    Stretch,
}

/// One layer of a scene: where a source's frames are drawn.
#[derive(Clone)]
pub struct Layer {
    pub slot: Arc<FrameSlot>,
    pub rect: NormRect,
    pub fit: Fit,
    pub visible: bool,
}

impl Layer {
    /// A source drawn over the whole picture.
    pub fn full(slot: Arc<FrameSlot>) -> Self {
        Self {
            slot,
            rect: NormRect::FULL,
            fit: Fit::Contain,
            visible: true,
        }
    }
}

/// A pixel rectangle, always with even sides and offsets (NV12 needs them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

fn even(value: u32) -> u32 {
    value & !1
}

/// Which part of a `content`-sized picture is drawn (the first rectangle) and
/// where in the `out`-sized output (the second).
pub fn layer_geometry(
    rect: NormRect,
    fit: Fit,
    content: (u32, u32),
    out: (u32, u32),
) -> (PixelRect, PixelRect) {
    let (out_w, out_h) = (out.0.max(2), out.1.max(2));
    let (content_w, content_h) = (content.0.max(2), content.1.max(2));
    let clamp01 = |v: f32| v.clamp(0.0, 1.0);
    let x0 = even((clamp01(rect.x) * out_w as f32).round() as u32).min(out_w - 2);
    let y0 = even((clamp01(rect.y) * out_h as f32).round() as u32).min(out_h - 2);
    let x1 = (clamp01(rect.x + rect.w) * out_w as f32).round() as u32;
    let y1 = (clamp01(rect.y + rect.h) * out_h as f32).round() as u32;
    let bw = even(x1.saturating_sub(x0)).clamp(2, out_w - x0);
    let bh = even(y1.saturating_sub(y0)).clamp(2, out_h - y0);
    let whole = PixelRect {
        x: 0,
        y: 0,
        w: even(content_w),
        h: even(content_h),
    };
    match fit {
        Fit::Stretch => (
            whole,
            PixelRect {
                x: x0,
                y: y0,
                w: bw,
                h: bh,
            },
        ),
        Fit::Contain => {
            let (dx, dy, w, h) = fit_in(content_w, content_h, bw, bh);
            (
                whole,
                PixelRect {
                    x: x0 + dx,
                    y: y0 + dy,
                    w,
                    h,
                },
            )
        }
        Fit::Cover => {
            // Crop the picture to the rectangle's proportions, from the centre.
            let box_aspect = f64::from(bw) / f64::from(bh);
            let content_aspect = f64::from(content_w) / f64::from(content_h);
            let (sw, sh) = if content_aspect > box_aspect {
                (
                    even((f64::from(content_h) * box_aspect).round() as u32).clamp(2, content_w),
                    even(content_h),
                )
            } else {
                (
                    even(content_w),
                    even((f64::from(content_w) / box_aspect).round() as u32).clamp(2, content_h),
                )
            };
            (
                PixelRect {
                    x: even((content_w - sw) / 2),
                    y: even((content_h - sh) / 2),
                    w: sw,
                    h: sh,
                },
                PixelRect {
                    x: x0,
                    y: y0,
                    w: bw,
                    h: bh,
                },
            )
        }
    }
}

fn rect_of(r: PixelRect) -> RECT {
    RECT {
        left: r.x as i32,
        top: r.y as i32,
        right: (r.x + r.w) as i32,
        bottom: (r.y + r.h) as i32,
    }
}

fn failed() -> windows::core::Error {
    windows::core::Error::from(E_FAIL)
}

/// Says which step failed, keeping the original error code.
fn step(what: &'static str) -> impl Fn(windows::core::Error) -> windows::core::Error {
    move |e| windows::core::Error::new(e.code(), format!("{what}: {}", e.message()))
}

/// The newest frame of one source, as a texture the video processor can read.
struct Input {
    texture: ID3D11Texture2D,
    view: ID3D11VideoProcessorInputView,
    width: u32,
    height: u32,
    format: PixelFormat,
}

/// Draws layers into an NV12 picture. Used from one thread at a time.
pub struct Compositor {
    device: ID3D11Device,
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
    max_layers: usize,
    /// One slot per layer position; kept between frames so a quiet source
    /// stays on screen.
    inputs: Vec<Option<Input>>,
}

// SAFETY: the device is created with multithread protection and the compositor
// is only ever used by one thread at a time (it is moved into that thread).
unsafe impl Send for Compositor {}

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
        // The output is both drawn into by the video processor and copied.
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
    texture.ok_or_else(failed)
}

impl Compositor {
    /// A compositor for an `out_w` × `out_h` picture (rounded down to even).
    pub fn new(out_w: u32, out_h: u32) -> windows::core::Result<Self> {
        let (out_w, out_h) = (even(out_w).max(2), even(out_h).max(2));
        let mut device = None;
        let mut context = None;
        // SAFETY: standard device creation with valid out-pointers.
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
            return Err(failed());
        };
        let multithread: ID3D11Multithread = context.cast()?;
        // SAFETY: plain setter.
        unsafe {
            let _ = multithread.SetMultithreadProtected(true);
        }
        let video_device: ID3D11VideoDevice = device.cast()?;
        let video_context: ID3D11VideoContext = context.cast()?;
        let rate = DXGI_RATIONAL {
            Numerator: 60,
            Denominator: 1,
        };
        let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: rate,
            InputWidth: out_w,
            InputHeight: out_h,
            OutputFrameRate: rate,
            OutputWidth: out_w,
            OutputHeight: out_h,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };
        // SAFETY: documented video-processor setup with valid descriptions.
        unsafe {
            let enumerator = video_device.CreateVideoProcessorEnumerator(&content)?;
            let mut caps = D3D11_VIDEO_PROCESSOR_CAPS::default();
            enumerator.GetVideoProcessorCaps(&mut caps)?;
            let max_layers = (caps.MaxInputStreams as usize).max(1);
            let processor = video_device.CreateVideoProcessor(&enumerator, 0)?;

            let output = nv12_texture(&device, out_w, out_h, false)?;
            let staging = nv12_texture(&device, out_w, out_h, true)?;
            let view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut output_view = None;
            video_device.CreateVideoProcessorOutputView(
                &output,
                &enumerator,
                &view_desc,
                Some(&mut output_view),
            )?;
            let output_view = output_view.ok_or_else(failed)?;

            // Everything is BT.709 limited range in and out: the colours the
            // capture card's NV12 already uses.
            let space = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
                _bitfield: (1 << 2) | (1 << 4),
            };
            video_context.VideoProcessorSetOutputColorSpace(&processor, &space);
            let full = rect_of(PixelRect {
                x: 0,
                y: 0,
                w: out_w,
                h: out_h,
            });
            video_context.VideoProcessorSetOutputTargetRect(&processor, true, Some(&full));
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

            Ok(Self {
                device,
                context,
                video_device,
                video_context,
                enumerator,
                processor,
                output_view,
                output,
                staging,
                out_w,
                out_h,
                max_layers,
                inputs: Vec::new(),
            })
        }
    }

    /// Most layers one scene can have on this graphics card.
    pub fn max_layers(&self) -> usize {
        self.max_layers
    }

    pub fn size(&self) -> (u32, u32) {
        (self.out_w, self.out_h)
    }

    /// Uploads `frame` as layer `index`'s picture.
    fn upload(&mut self, index: usize, frame: &Frame) -> windows::core::Result<()> {
        if self.inputs.len() <= index {
            self.inputs.resize_with(index + 1, || None);
        }
        let (width, height) = (even(frame.width).max(2), even(frame.height).max(2));
        let reuse = self.inputs[index]
            .as_ref()
            .is_some_and(|i| (i.width, i.height, i.format) == (width, height, frame.format));
        if !reuse {
            let format: DXGI_FORMAT = match frame.format {
                PixelFormat::Nv12 => DXGI_FORMAT_NV12,
                PixelFormat::Yuy2 => DXGI_FORMAT_YUY2,
            };
            let desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: format,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            // SAFETY: valid descriptions and out-pointers; the video-processor
            // objects belong to this device.
            unsafe {
                let mut texture = None;
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut texture))
                    .map_err(step("creating a source texture"))?;
                let texture: ID3D11Texture2D = texture.ok_or_else(failed)?;
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
                let mut view = None;
                self.video_device
                    .CreateVideoProcessorInputView(
                        &texture,
                        &self.enumerator,
                        &view_desc,
                        Some(&mut view),
                    )
                    .map_err(step("creating a source view"))?;
                self.inputs[index] = Some(Input {
                    texture,
                    view: view.ok_or_else(failed)?,
                    width,
                    height,
                    format: frame.format,
                });
            }
        }
        if let Some(input) = &self.inputs[index] {
            // SAFETY: the frame holds a complete picture of the declared
            // layout (the capture paths only publish complete buffers) and
            // outlives the call; planar formats take both planes in one go.
            unsafe {
                self.context.UpdateSubresource(
                    &input.texture,
                    0,
                    None,
                    frame.data.as_ptr().cast(),
                    frame.stride,
                    0,
                );
            }
        }
        Ok(())
    }

    /// Draws `layers` (bottom first) and publishes the picture to `out`.
    ///
    /// Layers whose source has not published yet are skipped. Returns how many
    /// layers were drawn.
    pub fn compose(
        &mut self,
        layers: &[Layer],
        out: &FrameSlot,
        timestamp: i64,
    ) -> windows::core::Result<usize> {
        // Fresh pictures first; a layer keeps its previous one otherwise.
        for (index, layer) in layers.iter().enumerate() {
            if let Some(frame) = layer.slot.take_latest() {
                let result = self.upload(index, &frame);
                layer.slot.recycle(frame);
                result?;
            }
        }
        // Layers beyond the card's limit are ignored rather than failing.
        let shown: Vec<(usize, &Layer)> = layers
            .iter()
            .enumerate()
            .filter(|(i, l)| l.visible && self.inputs.get(*i).is_some_and(Option::is_some))
            .take(self.max_layers)
            .collect();

        // SAFETY: video-processor calls on resources of this device. The
        // streams hold extra references to the views, released right after.
        let mut streams = Vec::with_capacity(shown.len());
        unsafe {
            for (stream, (index, layer)) in shown.iter().enumerate() {
                let Some(input) = self.inputs[*index].as_ref() else {
                    continue;
                };
                let (source, dest) = layer_geometry(
                    layer.rect,
                    layer.fit,
                    (input.width, input.height),
                    (self.out_w, self.out_h),
                );
                let n = stream as u32;
                let space = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
                    _bitfield: (1 << 2) | (1 << 4),
                };
                self.video_context
                    .VideoProcessorSetStreamColorSpace(&self.processor, n, &space);
                self.video_context.VideoProcessorSetStreamFrameFormat(
                    &self.processor,
                    n,
                    D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                );
                self.video_context.VideoProcessorSetStreamSourceRect(
                    &self.processor,
                    n,
                    true,
                    Some(&rect_of(source)),
                );
                self.video_context.VideoProcessorSetStreamDestRect(
                    &self.processor,
                    n,
                    true,
                    Some(&rect_of(dest)),
                );
                streams.push(D3D11_VIDEO_PROCESSOR_STREAM {
                    Enable: BOOL(1),
                    pInputSurface: ManuallyDrop::new(Some(input.view.clone())),
                    ..Default::default()
                });
            }
            let result = self.video_context.VideoProcessorBlt(
                &self.processor,
                &self.output_view,
                0,
                &streams,
            );
            for stream in streams {
                drop(ManuallyDrop::into_inner(stream.pInputSurface));
            }
            result.map_err(step("drawing the layers"))?;
            self.publish(out, timestamp)?;
        }
        Ok(shown.len())
    }

    /// Copies the output picture to the CPU and publishes it.
    ///
    /// # Safety
    /// Must be called from the thread that owns the compositor.
    unsafe fn publish(&mut self, out: &FrameSlot, timestamp: i64) -> windows::core::Result<()> {
        self.context.CopyResource(&self.staging, &self.output);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        self.context
            .Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let (w, h) = (self.out_w as usize, self.out_h as usize);
        let expected = PixelFormat::Nv12.buffer_len(w, h);
        let mut frame = out
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
        out.publish(frame);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The compositor thread
// ---------------------------------------------------------------------------

/// What the compositor thread reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneStatus {
    Starting,
    Running,
    Failed(String),
}

/// Runs a [`Compositor`] on a thread of its own while it is alive, drawing a
/// new picture whenever a visible source has a new frame (or the layers
/// change).
pub struct SceneCompositor {
    layers: Arc<Mutex<(Vec<Layer>, bool)>>,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<SceneStatus>>,
    thread: Option<JoinHandle<()>>,
}

/// How long the thread waits for the bottom layer before it looks at the
/// others (the camera can be the only thing moving).
const POLL: Duration = Duration::from_millis(4);

/// The most pictures per second the compositor makes. Two sources of 60 fps
/// would otherwise add up to 120 pictures a second, which nobody can use.
pub const OUTPUT_FPS: u32 = 60;

impl SceneCompositor {
    pub fn start(layers: Vec<Layer>, out: Arc<FrameSlot>, width: u32, height: u32) -> Self {
        let layers = Arc::new(Mutex::new((layers, true)));
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(SceneStatus::Starting));
        let thread = {
            let (layers, stop, status) =
                (Arc::clone(&layers), Arc::clone(&stop), Arc::clone(&status));
            thread::Builder::new()
                .name("scene-compositor".into())
                .spawn(move || {
                    // SAFETY: plain timer-resolution requests, undone below.
                    unsafe {
                        timeBeginPeriod(1);
                    }
                    let result = run(&layers, &out, width, height, &stop, &status);
                    // SAFETY: matches the call above.
                    unsafe {
                        timeEndPeriod(1);
                    }
                    if let Err(e) = result {
                        set_status(&status, SceneStatus::Failed(e.message()));
                    }
                })
                .ok()
        };
        if thread.is_none() {
            set_status(
                &status,
                SceneStatus::Failed("could not start the compositor thread".into()),
            );
        }
        Self {
            layers,
            stop,
            status,
            thread,
        }
    }

    /// Replaces the layers (a scene change or an edit).
    pub fn set_layers(&self, layers: Vec<Layer>) {
        *self.layers.lock().unwrap_or_else(|e| e.into_inner()) = (layers, true);
    }

    pub fn status(&self) -> SceneStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Drop for SceneCompositor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let deadline = Instant::now() + Duration::from_secs(3);
            while !thread.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(5));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

fn set_status(status: &Mutex<SceneStatus>, value: SceneStatus) {
    *status.lock().unwrap_or_else(|e| e.into_inner()) = value;
}

fn run(
    layers: &Mutex<(Vec<Layer>, bool)>,
    out: &FrameSlot,
    width: u32,
    height: u32,
    stop: &AtomicBool,
    status: &Mutex<SceneStatus>,
) -> windows::core::Result<()> {
    let mut compositor = Compositor::new(width, height)?;
    set_status(status, SceneStatus::Running);
    let started = Instant::now();
    let mut seen: Vec<u64> = Vec::new();
    let mut last_picture = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let (current, changed) = {
            let mut guard = layers.lock().unwrap_or_else(|e| e.into_inner());
            let changed = std::mem::take(&mut guard.1);
            (guard.0.clone(), changed)
        };
        // Wake on the first visible layer; the rest are polled.
        let wait_on = current.iter().find(|l| l.visible);
        let fresh = match wait_on {
            Some(layer) => layer.slot.wait_for_frame(POLL),
            None => {
                thread::sleep(POLL);
                false
            }
        };
        seen.resize(current.len(), 0);
        let mut news = fresh || changed;
        for (index, layer) in current.iter().enumerate() {
            let published = layer.slot.published();
            if layer.visible && published != seen[index] {
                news = true;
            }
            seen[index] = published;
        }
        if !news {
            continue;
        }
        // At most one picture per period; the newest frames win meanwhile.
        let period = Duration::from_secs_f64(1.0 / f64::from(OUTPUT_FPS));
        let due = last_picture + period;
        let now = Instant::now();
        if now < due {
            thread::sleep(due - now);
        }
        last_picture = Instant::now();
        let timestamp = (started.elapsed().as_nanos() / 100) as i64;
        compositor.compose(&current, out, timestamp)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT: (u32, u32) = (1920, 1080);

    #[test]
    fn a_full_layer_fills_the_picture() {
        let (src, dest) = layer_geometry(NormRect::FULL, Fit::Contain, (1920, 1080), OUT);
        assert_eq!(
            src,
            PixelRect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080
            }
        );
        assert_eq!(dest, src);
    }

    #[test]
    fn contain_keeps_the_proportion_with_bars() {
        // A 4:3 picture in a 16:9 output: pillarboxed, centred.
        let (_, dest) = layer_geometry(NormRect::FULL, Fit::Contain, (1440, 1080), OUT);
        assert_eq!((dest.w, dest.h), (1440, 1080));
        assert_eq!(dest.x, 240);
        assert_eq!(dest.y, 0);
    }

    #[test]
    fn stretch_ignores_the_proportion() {
        let (src, dest) = layer_geometry(NormRect::FULL, Fit::Stretch, (1440, 1080), OUT);
        assert_eq!((src.w, src.h), (1440, 1080));
        assert_eq!((dest.w, dest.h), (1920, 1080));
    }

    #[test]
    fn cover_crops_from_the_centre() {
        // A wide picture into a square box: the sides are cut.
        let rect = NormRect::new(0.0, 0.0, 0.5625, 1.0); // 1080 x 1080 px
        let (src, dest) = layer_geometry(rect, Fit::Cover, (1920, 1080), OUT);
        assert_eq!((dest.w, dest.h), (1080, 1080));
        assert_eq!((src.w, src.h), (1080, 1080));
        assert_eq!(src.x, 420);
        assert_eq!(src.y, 0);
    }

    #[test]
    fn a_corner_camera_lands_in_the_corner() {
        // 25 % wide, 16:9, bottom right with a small margin.
        let rect = NormRect::new(0.73, 0.70, 0.25, 0.25 * 16.0 / 9.0 * 9.0 / 16.0);
        let (_, dest) = layer_geometry(rect, Fit::Contain, (1280, 720), OUT);
        assert!(dest.x >= 1400 && dest.x + dest.w <= 1920);
        assert!(dest.y >= 750 && dest.y + dest.h <= 1080);
        assert_eq!(dest.x % 2, 0);
        assert_eq!(dest.y % 2, 0);
        assert_eq!(dest.w % 2, 0);
        assert_eq!(dest.h % 2, 0);
    }

    #[test]
    fn rectangles_outside_the_picture_are_pulled_back_in() {
        let rect = NormRect::new(0.9, 0.9, 0.5, 0.5);
        let (_, dest) = layer_geometry(rect, Fit::Stretch, (640, 360), OUT);
        assert!(dest.x + dest.w <= 1920 && dest.y + dest.h <= 1080);
        assert!(dest.w >= 2 && dest.h >= 2);
        let (_, dest) = layer_geometry(
            NormRect::new(-1.0, -1.0, 0.0, 0.0),
            Fit::Stretch,
            (2, 2),
            OUT,
        );
        assert_eq!((dest.x, dest.y), (0, 0));
        assert!(dest.w >= 2 && dest.h >= 2);
    }

    /// A solid NV12 picture of the given luma.
    fn solid(width: u32, height: u32, luma: u8) -> Frame {
        let (w, h) = (width as usize, height as usize);
        let mut data = vec![128u8; PixelFormat::Nv12.buffer_len(w, h)];
        data[..w * h].fill(luma);
        Frame {
            width,
            height,
            stride: width,
            format: PixelFormat::Nv12,
            data,
            timestamp: 0,
            arrived: Instant::now(),
        }
    }

    #[test]
    fn two_layers_are_drawn_in_order_and_in_place() {
        let Ok(mut compositor) = Compositor::new(640, 360) else {
            eprintln!("no video processor on this machine; skipping");
            return;
        };
        let (back, front) = (Arc::new(FrameSlot::new()), Arc::new(FrameSlot::new()));
        back.publish(solid(640, 360, 100));
        front.publish(solid(320, 180, 220));
        let layers = vec![
            Layer::full(Arc::clone(&back)),
            Layer {
                slot: Arc::clone(&front),
                rect: NormRect::new(0.5, 0.5, 0.5, 0.5),
                fit: Fit::Stretch,
                visible: true,
            },
        ];
        let out = FrameSlot::new();
        let drawn = compositor.compose(&layers, &out, 0).expect("compose");
        assert_eq!(drawn, 2);
        let frame = out.take_latest().expect("a picture");
        let luma = |x: usize, y: usize| i32::from(frame.data[y * 640 + x]);
        // The back layer shows where the front one is not...
        assert!((luma(100, 100) - 100).abs() <= 6, "back {}", luma(100, 100));
        // ...and the front one covers the bottom-right quarter.
        assert!(
            (luma(500, 300) - 220).abs() <= 6,
            "front {}",
            luma(500, 300)
        );

        // A quiet source stays on screen, and a hidden layer disappears.
        let mut layers = layers;
        layers[1].visible = false;
        compositor.compose(&layers, &out, 1).expect("compose again");
        let frame = out.take_latest().expect("a picture");
        assert!((i32::from(frame.data[300 * 640 + 500]) - 100).abs() <= 6);
    }

    #[test]
    fn a_yuy2_source_can_be_a_layer_too() {
        let Ok(mut compositor) = Compositor::new(640, 360) else {
            eprintln!("no video processor on this machine; skipping");
            return;
        };
        // Packed Y0 U Y1 V, luma 200 everywhere.
        let (w, h) = (320u32, 180u32);
        let mut data = vec![0u8; PixelFormat::Yuy2.buffer_len(w as usize * 2, h as usize)];
        for quad in data.as_chunks_mut::<4>().0 {
            *quad = [200, 128, 200, 128];
        }
        let slot = Arc::new(FrameSlot::new());
        slot.publish(Frame {
            width: w,
            height: h,
            stride: w * 2,
            format: PixelFormat::Yuy2,
            data,
            timestamp: 0,
            arrived: Instant::now(),
        });
        let out = FrameSlot::new();
        let drawn = compositor
            .compose(&[Layer::full(slot)], &out, 0)
            .expect("compose");
        assert_eq!(drawn, 1);
        let frame = out.take_latest().expect("a picture");
        let luma = i32::from(frame.data[180 * 640 + 320]);
        assert!((luma - 200).abs() <= 6, "luma {luma}");
    }
}
