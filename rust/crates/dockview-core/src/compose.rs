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
use std::thread;
use std::time::{Duration, Instant};

use windows::core::{Interface, BOOL};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice,
    ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView,
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_TEX2D_VPIV, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CAPS,
    D3D11_VIDEO_PROCESSOR_COLOR_SPACE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0,
    D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
    D3D11_VPIV_DIMENSION_TEXTURE2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_NV12, DXGI_FORMAT_YUY2, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};

use crate::frame::{Frame, FrameSlot, PixelFormat};
use crate::gpu::{create_device, even, failed, Nv12Target};
use crate::screen::fit_in;
use crate::worker::{StatusCell, Worker};

pub use crate::scene::{Fit, NormRect};

/// One layer of a scene: where a source's frames are drawn.
#[derive(Clone)]
pub struct Layer {
    pub slot: Arc<FrameSlot>,
    pub rect: NormRect,
    pub fit: Fit,
    pub visible: bool,
    /// 1 is fully drawn, 0 invisible; in between it lets what is below show
    /// through (scene transitions).
    pub opacity: f32,
}

impl Layer {
    /// A source drawn over the whole picture.
    pub fn full(slot: Arc<FrameSlot>) -> Self {
        Self {
            slot,
            rect: NormRect::FULL,
            fit: Fit::Contain,
            visible: true,
            opacity: 1.0,
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
    target: Nv12Target,
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

impl Compositor {
    /// A compositor for an `out_w` × `out_h` picture (rounded down to even).
    pub fn new(out_w: u32, out_h: u32) -> windows::core::Result<Self> {
        let (out_w, out_h) = (even(out_w).max(2), even(out_h).max(2));
        let (device, context) = create_device()?;
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

            let target =
                Nv12Target::new(&device, &context, &video_device, &enumerator, out_w, out_h)?;

            // Everything is BT.709 limited range in and out: the colours the
            // capture card's NV12 already uses.
            let space = D3D11_VIDEO_PROCESSOR_COLOR_SPACE {
                _bitfield: (1 << 2) | (1 << 4),
            };
            video_context.VideoProcessorSetOutputColorSpace(&processor, &space);
            target.prepare(&video_context, &processor);

            Ok(Self {
                device,
                context,
                video_device,
                video_context,
                enumerator,
                processor,
                target,
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
                self.video_context.VideoProcessorSetStreamAlpha(
                    &self.processor,
                    n,
                    layer.opacity < 1.0,
                    layer.opacity.clamp(0.0, 1.0),
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
                self.target.view(),
                0,
                &streams,
            );
            for stream in streams {
                drop(ManuallyDrop::into_inner(stream.pInputSurface));
            }
            result.map_err(step("drawing the layers"))?;
            self.target.publish(out, timestamp)?;
        }
        Ok(shown.len())
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
    status: Arc<StatusCell<SceneStatus>>,
    _worker: Worker,
}

/// How long dropping the compositor waits for its thread.
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

/// How long the thread waits for the bottom layer before it looks at the
/// others (the camera can be the only thing moving).
const POLL: Duration = Duration::from_millis(4);

/// The most pictures per second the compositor makes. Two sources of 60 fps
/// would otherwise add up to 120 pictures a second, which nobody can use.
pub const OUTPUT_FPS: u32 = 60;

impl SceneCompositor {
    pub fn start(layers: Vec<Layer>, out: Arc<FrameSlot>, width: u32, height: u32) -> Self {
        let layers = Arc::new(Mutex::new((layers, true)));
        let status = Arc::new(StatusCell::new(SceneStatus::Starting));
        let mut worker = Worker::new(STOP_TIMEOUT);
        let (thread_layers, thread_status) = (Arc::clone(&layers), Arc::clone(&status));
        let started = worker.spawn("scene-compositor", move |stop| {
            // SAFETY: plain timer-resolution requests, undone below.
            unsafe {
                timeBeginPeriod(1);
            }
            let result = run(&thread_layers, &out, width, height, stop, &thread_status);
            // SAFETY: matches the call above.
            unsafe {
                timeEndPeriod(1);
            }
            if let Err(e) = result {
                thread_status.set(SceneStatus::Failed(e.message()));
            }
        });
        if !started {
            status.set(SceneStatus::Failed(
                "could not start the compositor thread".into(),
            ));
        }
        Self {
            layers,
            status,
            _worker: worker,
        }
    }

    /// Replaces the layers (a scene change or an edit).
    pub fn set_layers(&self, layers: Vec<Layer>) {
        *self.layers.lock().unwrap_or_else(|e| e.into_inner()) = (layers, true);
    }

    pub fn status(&self) -> SceneStatus {
        self.status.get()
    }
}

fn run(
    layers: &Mutex<(Vec<Layer>, bool)>,
    out: &FrameSlot,
    width: u32,
    height: u32,
    stop: &AtomicBool,
    status: &StatusCell<SceneStatus>,
) -> windows::core::Result<()> {
    let mut compositor = Compositor::new(width, height)?;
    status.set(SceneStatus::Running);
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
                opacity: 1.0,
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
    fn a_half_transparent_layer_lets_the_one_below_show_through() {
        let Ok(mut compositor) = Compositor::new(640, 360) else {
            eprintln!("no video processor on this machine; skipping");
            return;
        };
        let (back, front) = (Arc::new(FrameSlot::new()), Arc::new(FrameSlot::new()));
        back.publish(solid(640, 360, 60));
        front.publish(solid(640, 360, 200));
        let mut layers = vec![Layer::full(back), Layer::full(front)];
        layers[1].opacity = 0.5;
        let out = FrameSlot::new();
        compositor.compose(&layers, &out, 0).expect("compose");
        let frame = out.take_latest().expect("a picture");
        let luma = i32::from(frame.data[100 * 640 + 100]);
        // Halfway between 60 and 200 (with some room for the colour maths).
        assert!((luma - 130).abs() <= 12, "mixed luma {luma}");
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
