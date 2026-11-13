//! Direct3D 11 renderer: flip-model swap chain with a frame-latency waitable
//! object, YUV → RGB conversion on the GPU.

use std::ffi::c_void;

use dockview_domain::frame::{Frame, PixelFormat};
use dockview_domain::layout::Rect;
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

mod draw;
mod gpu_timer;
mod shader;

use gpu_timer::GpuTimer;
use shader::create_sampler;
pub(crate) use shader::{blob_bytes, compile};

const SHADER_SOURCE: &str = include_str!("../render.hlsl");

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
    /// Where the video goes; `None` is the whole window.
    video_area: Option<Rect>,
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
                video_area: None,
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

    /// Fits the video into `area` (physical pixels) instead of the whole
    /// window, for example to leave room for a side panel. `None` goes back
    /// to the whole window.
    pub fn set_video_area(&mut self, area: Option<Rect>) {
        self.video_area = area;
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
}

fn missing() -> Error {
    Error::new(E_FAIL, "D3D11 returned no object")
}

/// Largest rectangle with the video's aspect ratio that fits the window,
/// centred. Returns `(x, y, width, height)` in pixels.
pub fn letterbox(window: (u32, u32), video: (u32, u32)) -> (f32, f32, f32, f32) {
    let whole = Rect {
        x: 0.0,
        y: 0.0,
        w: window.0 as f32,
        h: window.1 as f32,
    };
    letterbox_in(whole, video)
}

/// Like [`letterbox`], inside `area` instead of the whole window.
pub fn letterbox_in(area: Rect, video: (u32, u32)) -> (f32, f32, f32, f32) {
    let (vw, vh) = (video.0.max(1) as f32, video.1.max(1) as f32);
    let scale = (area.w / vw).min(area.h / vh);
    let (w, h) = (vw * scale, vh * scale);
    (
        area.x + (area.w - w) / 2.0,
        area.y + (area.h - h) / 2.0,
        w,
        h,
    )
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
    fn the_video_is_centred_inside_an_area_that_does_not_start_at_zero() {
        // A 1600x1080 area on the left of a 1920x1080 window (panel on the right).
        let area = Rect {
            x: 0.0,
            y: 0.0,
            w: 1600.0,
            h: 1080.0,
        };
        let (x, y, w, h) = letterbox_in(area, (1920, 1080));
        assert!((w - 1600.0).abs() < 0.01 && x.abs() < 0.01);
        assert!((h - 900.0).abs() < 0.01 && (y - 90.0).abs() < 0.01);
        // An area offset from the origin keeps the offset.
        let offset = Rect {
            x: 320.0,
            y: 0.0,
            w: 1600.0,
            h: 1080.0,
        };
        assert!((letterbox_in(offset, (1920, 1080)).0 - 320.0).abs() < 0.01);
    }

    #[test]
    fn tall_window_gets_letterboxed() {
        let (x, y, w, h) = letterbox((1000, 1000), (1920, 1080));
        assert!((w - 1000.0).abs() < 0.01 && x.abs() < 0.01);
        assert!((h - 562.5).abs() < 0.01);
        assert!((y - 218.75).abs() < 0.01);
    }
}
