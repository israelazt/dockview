//! Direct3D 11 renderer: flip-model swap chain with a frame-latency waitable
//! object, YUV → RGB conversion on the GPU.

use std::ffi::c_void;

use dockview_domain::frame::{Frame, PixelFormat};
use windows::core::{s, Error, Interface, Result};
use windows::Win32::Foundation::{E_FAIL, HANDLE, HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCompile, D3DCOMPILE_OPTIMIZATION_LEVEL3};
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader,
    ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11Texture2D,
    ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_SHADER_RESOURCE, D3D11_BUFFER_DESC,
    D3D11_COMPARISON_NEVER, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_FILTER_MIN_MAG_MIP_POINT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SAMPLER_DESC, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
    D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Query, D3D11_QUERY_DATA_TIMESTAMP_DISJOINT, D3D11_QUERY_DESC, D3D11_QUERY_TIMESTAMP,
    D3D11_QUERY_TIMESTAMP_DISJOINT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_R8G8_UNORM, DXGI_FORMAT_R8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, IDXGIFactory2, IDXGISwapChain2, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_SWAP_EFFECT_FLIP_DISCARD,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::System::Threading::WaitForSingleObjectEx;

const SHADER_SOURCE: &str = include_str!("render.hlsl");

/// Swap-chain flags. Must be identical at creation and in `ResizeBuffers`.
const SWAP_CHAIN_FLAGS: i32 = DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0;

/// Texture sampling used when scaling the video to the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    Linear,
}

/// Textures holding the most recently uploaded frame.
struct VideoTextures {
    format: PixelFormat,
    width: u32,
    height: u32,
    stride: u32,
    /// Luma (NV12) or packed texels (YUY2).
    primary: ID3D11ShaderResourceView,
    primary_tex: ID3D11Texture2D,
    /// Interleaved chroma (NV12 only).
    chroma: Option<(ID3D11Texture2D, ID3D11ShaderResourceView)>,
}

/// Measures how long the GPU spends on each frame, using timestamp queries.
///
/// Results arrive a few frames late (the GPU runs behind the CPU), so queries
/// are kept in a small ring and read back without ever stalling.
struct GpuTimer {
    sets: Vec<QuerySet>,
    next: usize,
    active: Option<usize>,
    /// Smoothed GPU time per frame, in milliseconds.
    smoothed_ms: Option<f32>,
}

struct QuerySet {
    disjoint: ID3D11Query,
    start: ID3D11Query,
    end: ID3D11Query,
    in_flight: bool,
}

impl GpuTimer {
    const RING: usize = 4;

    fn new(device: &ID3D11Device) -> Result<Self> {
        let make = |kind| -> Result<ID3D11Query> {
            let desc = D3D11_QUERY_DESC {
                Query: kind,
                MiscFlags: 0,
            };
            let mut query = None;
            // SAFETY: valid query description and out-pointer.
            unsafe { device.CreateQuery(&desc, Some(&mut query))? };
            query.ok_or_else(missing)
        };
        let mut sets = Vec::with_capacity(Self::RING);
        for _ in 0..Self::RING {
            sets.push(QuerySet {
                disjoint: make(D3D11_QUERY_TIMESTAMP_DISJOINT)?,
                start: make(D3D11_QUERY_TIMESTAMP)?,
                end: make(D3D11_QUERY_TIMESTAMP)?,
                in_flight: false,
            });
        }
        Ok(Self {
            sets,
            next: 0,
            active: None,
            smoothed_ms: None,
        })
    }

    fn begin(&mut self, context: &ID3D11DeviceContext) {
        let index = self.next;
        if self.sets[index].in_flight {
            self.active = None; // the GPU is still behind: skip this frame
            return;
        }
        let set = &self.sets[index];
        // SAFETY: the queries belong to the same device as `context`.
        unsafe {
            context.Begin(&set.disjoint);
            context.End(&set.start);
        }
        self.active = Some(index);
    }

    fn end(&mut self, context: &ID3D11DeviceContext) {
        let Some(index) = self.active.take() else {
            return;
        };
        let set = &mut self.sets[index];
        // SAFETY: see `begin`.
        unsafe {
            context.End(&set.end);
            context.End(&set.disjoint);
        }
        set.in_flight = true;
        self.next = (index + 1) % Self::RING;
    }

    /// Reads back every finished query set.
    fn collect(&mut self, context: &ID3D11DeviceContext) {
        for set in self.sets.iter_mut().filter(|s| s.in_flight) {
            // GetData reports "not ready yet" as a success code, so a zero
            // frequency (the value we pre-load) means nothing was written.
            let mut disjoint = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
            let (mut start, mut end) = (0u64, 0u64);
            // SAFETY: every out-pointer matches the size passed with it.
            let ready = unsafe {
                context
                    .GetData(
                        &set.disjoint,
                        Some((&mut disjoint as *mut D3D11_QUERY_DATA_TIMESTAMP_DISJOINT).cast()),
                        std::mem::size_of::<D3D11_QUERY_DATA_TIMESTAMP_DISJOINT>() as u32,
                        0,
                    )
                    .is_ok()
                    && disjoint.Frequency != 0
                    && context
                        .GetData(&set.start, Some((&mut start as *mut u64).cast()), 8, 0)
                        .is_ok()
                    && context
                        .GetData(&set.end, Some((&mut end as *mut u64).cast()), 8, 0)
                        .is_ok()
                    && end != 0
            };
            if !ready {
                continue;
            }
            set.in_flight = false;
            if !disjoint.Disjoint.as_bool() && end >= start {
                let ms = (end - start) as f64 / disjoint.Frequency as f64 * 1000.0;
                self.smoothed_ms = Some(match self.smoothed_ms {
                    Some(previous) => previous * 0.9 + ms as f32 * 0.1,
                    None => ms as f32,
                });
            }
        }
    }
}

/// Callback that draws on top of the video: device, context, target size.
pub type Overlay<'a> = dyn FnMut(&ID3D11Device, &ID3D11DeviceContext, (u32, u32)) + 'a;

/// A captured frame in RGBA order.
pub struct Screenshot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain2,
    target: Option<ID3D11RenderTargetView>,
    waitable: HANDLE,
    vertex_shader: ID3D11VertexShader,
    ps_nv12: ID3D11PixelShader,
    ps_yuy2: ID3D11PixelShader,
    params: ID3D11Buffer,
    sampler_linear: ID3D11SamplerState,
    sampler_point: ID3D11SamplerState,
    filter: Filter,
    video: Option<VideoTextures>,
    size: (u32, u32),
    gpu_timer: Option<GpuTimer>,
}

impl Renderer {
    /// Creates the device, swap chain and shaders for `hwnd`.
    pub fn new(hwnd: HWND, width: u32, height: u32) -> Result<Self> {
        // SAFETY: standard D3D11/DXGI initialisation with valid descriptors.
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )?;
            let device = device.ok_or_else(|| Error::new(E_FAIL, "no D3D11 device"))?;
            let context = context.ok_or_else(|| Error::new(E_FAIL, "no D3D11 context"))?;

            let factory: IDXGIFactory2 = device.cast::<IDXGIDevice>()?.GetAdapter()?.GetParent()?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width.max(1),
                Height: height.max(1),
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                Flags: SWAP_CHAIN_FLAGS as u32,
                ..Default::default()
            };
            let swap_chain: IDXGISwapChain2 = factory
                .CreateSwapChainForHwnd(&device, hwnd, &desc, None, None)?
                .cast()?;
            // At most one frame may be queued: this is what bounds latency.
            swap_chain.SetMaximumFrameLatency(1)?;
            let waitable = swap_chain.GetFrameLatencyWaitableObject();

            let vs_blob = compile(SHADER_SOURCE, "vs_main", s!("vs_5_0"))?;
            let nv12_blob = compile(SHADER_SOURCE, "ps_nv12", s!("ps_5_0"))?;
            let yuy2_blob = compile(SHADER_SOURCE, "ps_yuy2", s!("ps_5_0"))?;
            let mut vertex_shader = None;
            device.CreateVertexShader(blob_bytes(&vs_blob), None, Some(&mut vertex_shader))?;
            let mut ps_nv12 = None;
            device.CreatePixelShader(blob_bytes(&nv12_blob), None, Some(&mut ps_nv12))?;
            let mut ps_yuy2 = None;
            device.CreatePixelShader(blob_bytes(&yuy2_blob), None, Some(&mut ps_yuy2))?;

            let buffer_desc = D3D11_BUFFER_DESC {
                ByteWidth: 16,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                ..Default::default()
            };
            let mut params = None;
            device.CreateBuffer(&buffer_desc, None, Some(&mut params))?;

            let mut renderer = Self {
                sampler_linear: create_sampler(&device, D3D11_FILTER_MIN_MAG_MIP_LINEAR)?,
                sampler_point: create_sampler(&device, D3D11_FILTER_MIN_MAG_MIP_POINT)?,
                device,
                context,
                swap_chain,
                target: None,
                waitable,
                vertex_shader: vertex_shader.ok_or_else(missing)?,
                ps_nv12: ps_nv12.ok_or_else(missing)?,
                ps_yuy2: ps_yuy2.ok_or_else(missing)?,
                params: params.ok_or_else(missing)?,
                filter: Filter::Linear,
                video: None,
                size: (width.max(1), height.max(1)),
                gpu_timer: None,
            };
            renderer.gpu_timer = GpuTimer::new(&renderer.device).ok();
            renderer.create_target()?;
            Ok(renderer)
        }
    }

    pub fn set_filter(&mut self, filter: Filter) {
        self.filter = filter;
    }

    /// Blocks until the swap chain can accept another frame (≤ `timeout_ms`).
    pub fn wait_for_present_slot(&self, timeout_ms: u32) {
        // SAFETY: `waitable` is owned by the swap chain and valid for its life.
        unsafe {
            WaitForSingleObjectEx(self.waitable, timeout_ms, true);
        }
    }

    /// Resizes the back buffers. A zero-sized (minimised) window is ignored.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 || (width, height) == self.size {
            return Ok(());
        }
        // SAFETY: all views onto the back buffer are released before resizing.
        unsafe {
            self.target = None;
            self.context.ClearState();
            self.swap_chain.ResizeBuffers(
                0,
                width,
                height,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(SWAP_CHAIN_FLAGS),
            )?;
        }
        self.size = (width, height);
        self.create_target()
    }

    /// Copies `frame` into GPU textures, (re)creating them if the format or
    /// geometry changed.
    pub fn upload(&mut self, frame: &Frame) -> Result<()> {
        let needs_new = self.video.as_ref().is_none_or(|v| {
            (v.format, v.width, v.height, v.stride)
                != (frame.format, frame.width, frame.height, frame.stride)
        });
        if needs_new {
            self.video = Some(self.create_video_textures(frame)?);
        }
        let Some(video) = &self.video else {
            return Ok(());
        };

        // SAFETY: `frame.data` is large enough for the declared layout (the
        // capture path only publishes complete buffers) and outlives the call.
        unsafe {
            let base = frame.data.as_ptr().cast::<c_void>();
            self.context
                .UpdateSubresource(&video.primary_tex, 0, None, base, frame.stride, 0);
            if let Some((chroma_tex, _)) = &video.chroma {
                let plane = frame.stride as usize * frame.height as usize;
                let chroma = frame.data.as_ptr().add(plane).cast::<c_void>();
                self.context
                    .UpdateSubresource(chroma_tex, 0, None, chroma, frame.stride, 0);
            }
        }
        Ok(())
    }

    pub fn device(&self) -> &ID3D11Device {
        &self.device
    }

    /// Smoothed GPU time per frame in milliseconds, once measured.
    pub fn gpu_ms(&self) -> Option<f32> {
        self.gpu_timer.as_ref().and_then(|t| t.smoothed_ms)
    }

    /// Draws the current video (letterboxed), then lets `overlay` draw on top
    /// (it receives the device, context and render-target size), and presents.
    /// With `capture` set, also returns the finished image.
    pub fn draw(&mut self, overlay: &mut Overlay, capture: bool) -> Result<Option<Screenshot>> {
        let Some(target) = self.target.clone() else {
            return Ok(None);
        };
        // SAFETY: all resources are owned by `self` and alive for the draw.
        unsafe {
            if let Some(timer) = self.gpu_timer.as_mut() {
                timer.begin(&self.context);
            }
            self.context
                .OMSetRenderTargets(Some(&[Some(target.clone())]), None);
            self.context
                .ClearRenderTargetView(&target, &[0.0, 0.0, 0.0, 1.0]);

            if let Some(video) = &self.video {
                let (x, y, w, h) = letterbox(self.size, (video.width, video.height));
                self.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    TopLeftX: x,
                    TopLeftY: y,
                    Width: w,
                    Height: h,
                    MinDepth: 0.0,
                    MaxDepth: 1.0,
                }]));

                let (uv_scale_x, visible_w) = match video.format {
                    PixelFormat::Nv12 => (video.width as f32 / video.stride as f32, 0.0),
                    PixelFormat::Yuy2 => (
                        video.width as f32 / video.stride as f32 * 2.0,
                        video.width as f32 / 2.0,
                    ),
                };
                let params = [
                    uv_scale_x,
                    1.0,
                    if video.height >= 720 { 1.0 } else { 0.0 },
                    visible_w,
                ];
                self.context
                    .UpdateSubresource(&self.params, 0, None, params.as_ptr().cast(), 0, 0);

                let (shader, sampler) = match video.format {
                    PixelFormat::Nv12 => (&self.ps_nv12, self.sampler_for_filter()),
                    // Packed texels cannot be interpolated across pixel pairs.
                    PixelFormat::Yuy2 => (&self.ps_yuy2, &self.sampler_point),
                };
                let chroma = video.chroma.as_ref().map(|(_, srv)| srv.clone());
                self.context
                    .IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                self.context.VSSetShader(&self.vertex_shader, None);
                self.context.PSSetShader(shader, None);
                self.context
                    .PSSetShaderResources(0, Some(&[Some(video.primary.clone()), chroma]));
                self.context
                    .PSSetSamplers(0, Some(&[Some(sampler.clone())]));
                self.context
                    .PSSetConstantBuffers(0, Some(&[Some(self.params.clone())]));
                self.context.Draw(3, 0);
            }

            // The overlay owns the full window, whatever the video viewport was.
            self.context
                .OMSetRenderTargets(Some(&[Some(target.clone())]), None);
            overlay(&self.device, &self.context, self.size);
            if let Some(timer) = self.gpu_timer.as_mut() {
                timer.end(&self.context);
            }

            let shot = if capture {
                Some(self.read_back()?)
            } else {
                None
            };
            self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()?;
            if let Some(timer) = self.gpu_timer.as_mut() {
                timer.collect(&self.context);
            }
            Ok(shot)
        }
    }

    /// Copies the current back buffer to the CPU as RGBA.
    fn read_back(&self) -> Result<Screenshot> {
        // SAFETY: the staging texture is created with the back buffer's size
        // and format; rows are copied within the mapped pitch.
        unsafe {
            let back: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            back.GetDesc(&mut desc);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            desc.MiscFlags = 0;

            let mut staging = None;
            self.device
                .CreateTexture2D(&desc, None, Some(&mut staging))?;
            let staging = staging.ok_or_else(missing)?;
            self.context.CopyResource(&staging, &back);

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let (width, height) = (desc.Width as usize, desc.Height as usize);
            let mut rgba = Vec::with_capacity(width * height * 4);
            for row in 0..height {
                let line = std::slice::from_raw_parts(
                    mapped
                        .pData
                        .cast::<u8>()
                        .add(row * mapped.RowPitch as usize),
                    width * 4,
                );
                let (pixels, _) = line.as_chunks::<4>();
                for [b, g, r, _] in pixels {
                    rgba.extend_from_slice(&[*r, *g, *b, 255]); // BGRA → RGBA
                }
            }
            self.context.Unmap(&staging, 0);
            Ok(Screenshot {
                width: desc.Width,
                height: desc.Height,
                rgba,
            })
        }
    }

    fn sampler_for_filter(&self) -> &ID3D11SamplerState {
        match self.filter {
            Filter::Nearest => &self.sampler_point,
            Filter::Linear => &self.sampler_linear,
        }
    }

    fn create_target(&mut self) -> Result<()> {
        // SAFETY: buffer 0 of a flip-model swap chain is a valid render target.
        unsafe {
            let back_buffer: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
            let mut target = None;
            self.device
                .CreateRenderTargetView(&back_buffer, None, Some(&mut target))?;
            self.target = target;
        }
        Ok(())
    }

    fn create_video_textures(&self, frame: &Frame) -> Result<VideoTextures> {
        // Textures are as wide as the row stride so rows upload without
        // repacking; the shader crops to the visible width.
        let (primary_w, primary_fmt) = match frame.format {
            PixelFormat::Nv12 => (frame.stride, DXGI_FORMAT_R8_UNORM),
            PixelFormat::Yuy2 => (frame.stride / 4, DXGI_FORMAT_R8G8B8A8_UNORM),
        };
        let (primary_tex, primary) = self.create_texture(primary_w, frame.height, primary_fmt)?;
        let chroma = match frame.format {
            PixelFormat::Nv12 => Some(self.create_texture(
                frame.stride / 2,
                frame.height / 2,
                DXGI_FORMAT_R8G8_UNORM,
            )?),
            PixelFormat::Yuy2 => None,
        };
        Ok(VideoTextures {
            format: frame.format,
            width: frame.width,
            height: frame.height,
            stride: frame.stride,
            primary,
            primary_tex,
            chroma,
        })
    }

    fn create_texture(
        &self,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<(ID3D11Texture2D, ID3D11ShaderResourceView)> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width.max(1),
            Height: height.max(1),
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        // SAFETY: valid descriptor; resources are returned to the caller.
        unsafe {
            let mut texture = None;
            self.device
                .CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.ok_or_else(missing)?;
            let mut view = None;
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut view))?;
            Ok((texture, view.ok_or_else(missing)?))
        }
    }
}

fn missing() -> Error {
    Error::new(E_FAIL, "D3D11 returned no object")
}

/// # Safety
/// Calls into the D3D compiler with a valid source buffer.
pub(crate) unsafe fn compile(
    source: &str,
    entry: &str,
    target: windows::core::PCSTR,
) -> Result<ID3DBlob> {
    let entry = std::ffi::CString::new(entry).map_err(|_| missing())?;
    let mut code = None;
    let mut errors = None;
    let result = D3DCompile(
        source.as_ptr().cast(),
        source.len(),
        s!("render.hlsl"),
        None,
        None,
        windows::core::PCSTR(entry.as_ptr().cast()),
        target,
        D3DCOMPILE_OPTIMIZATION_LEVEL3,
        0,
        &mut code,
        Some(&mut errors),
    );
    if let Err(e) = result {
        let detail = errors
            .map(|b| String::from_utf8_lossy(blob_bytes(&b)).into_owned())
            .unwrap_or_default();
        return Err(Error::new(e.code(), detail));
    }
    code.ok_or_else(missing)
}

/// # Safety
/// The returned slice borrows the blob's buffer.
pub(crate) unsafe fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
}

/// # Safety
/// `device` must be a valid D3D11 device.
unsafe fn create_sampler(
    device: &ID3D11Device,
    filter: windows::Win32::Graphics::Direct3D11::D3D11_FILTER,
) -> Result<ID3D11SamplerState> {
    let desc = D3D11_SAMPLER_DESC {
        Filter: filter,
        AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
        ComparisonFunc: D3D11_COMPARISON_NEVER,
        MaxLOD: f32::MAX,
        ..Default::default()
    };
    let mut sampler = None;
    device.CreateSamplerState(&desc, Some(&mut sampler))?;
    sampler.ok_or_else(missing)
}

/// Largest rectangle with the video's aspect ratio that fits the window,
/// centred. Returns `(x, y, width, height)` in pixels.
pub fn letterbox(window: (u32, u32), video: (u32, u32)) -> (f32, f32, f32, f32) {
    let (ww, wh) = (window.0 as f32, window.1 as f32);
    let (vw, vh) = (video.0.max(1) as f32, video.1.max(1) as f32);
    let scale = (ww / vw).min(wh / vh);
    let (w, h) = (vw * scale, vh * scale);
    ((ww - w) / 2.0, (wh - h) / 2.0, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_aspect_fills_the_window() {
        assert_eq!(
            letterbox((1920, 1080), (1920, 1080)),
            (0.0, 0.0, 1920.0, 1080.0)
        );
        assert_eq!(
            letterbox((960, 540), (1920, 1080)),
            (0.0, 0.0, 960.0, 540.0)
        );
    }

    #[test]
    fn wide_window_gets_pillarboxed() {
        let (x, y, w, h) = letterbox((2000, 1000), (1600, 900));
        assert!((h - 1000.0).abs() < 0.01 && y.abs() < 0.01);
        assert!((w - 1777.78).abs() < 0.1);
        assert!((x - (2000.0 - w) / 2.0).abs() < 0.01);
    }

    #[test]
    fn tall_window_gets_letterboxed() {
        let (x, y, w, h) = letterbox((1000, 1000), (1920, 1080));
        assert!((w - 1000.0).abs() < 0.01 && x.abs() < 0.01);
        assert!((h - 562.5).abs() < 0.01);
        assert!((y - 218.75).abs() < 0.01);
    }
}
