//! The graphics of the selection overlay: a swap chain on the overlay window,
//! the frozen screenshot drawn as one picture, and a hook where the interface
//! (the dimming, the buttons) is drawn on top with the same engine the rest of
//! Dockview uses.

use std::ffi::c_void;

use dockview_domain::image::Image;
use windows::core::{s, Error, Interface, Result};
use windows::Win32::Foundation::{E_FAIL, HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCompile, D3DCOMPILE_OPTIMIZATION_LEVEL3};
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D_DRIVER_TYPE_HARDWARE,
    D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader,
    ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11Texture2D,
    ID3D11VertexShader, D3D11_BIND_SHADER_RESOURCE, D3D11_COMPARISON_NEVER,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_FILTER_MIN_MAG_MIP_POINT, D3D11_SAMPLER_DESC,
    D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP,
    D3D11_USAGE_IMMUTABLE, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, IDXGIFactory2, IDXGISwapChain1, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};

/// The picture is drawn with one triangle that covers the window.
const SHADER: &str = r"
struct VsOut { float4 pos : SV_Position; float2 uv : TEXCOORD0; };
VsOut vs_main(uint id : SV_VertexID) {
    VsOut o;
    float2 uv = float2((id << 1) & 2, id & 2);
    o.pos = float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
    o.uv = uv;
    return o;
}
Texture2D picture : register(t0);
SamplerState samp : register(s0);
float4 ps_main(VsOut i) : SV_Target { return float4(picture.Sample(samp, i.uv).rgb, 1.0); }
";

/// What is drawn over the picture: it gets the device, its context and the size
/// of the window in pixels.
pub type Chrome<'a> = dyn FnMut(&ID3D11Device, &ID3D11DeviceContext, (u32, u32)) + 'a;

pub struct OverlayGpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain1,
    target: ID3D11RenderTargetView,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    picture: Option<ID3D11ShaderResourceView>,
    size: (u32, u32),
}

fn missing() -> Error {
    Error::new(E_FAIL, "a Direct3D object was not created")
}

/// # Safety
/// Calls into the D3D compiler with a valid source buffer.
unsafe fn compile(entry: &str, target: windows::core::PCSTR) -> Result<ID3DBlob> {
    let entry = std::ffi::CString::new(entry).map_err(|_| missing())?;
    let mut code = None;
    let mut errors = None;
    let result = unsafe {
        D3DCompile(
            SHADER.as_ptr().cast(),
            SHADER.len(),
            s!("overlay.hlsl"),
            None,
            None,
            windows::core::PCSTR(entry.as_ptr().cast()),
            target,
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(e) = result {
        let detail = errors
            .map(|b| unsafe { String::from_utf8_lossy(blob_bytes(&b)).into_owned() })
            .unwrap_or_default();
        return Err(Error::new(e.code(), detail));
    }
    code.ok_or_else(missing)
}

/// # Safety
/// The returned slice borrows the blob's buffer.
unsafe fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    }
}

impl OverlayGpu {
    /// The device and swap chain for `hwnd`, `width` by `height` pixels.
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
            let device = device.ok_or_else(missing)?;
            let context = context.ok_or_else(missing)?;
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
                ..Default::default()
            };
            let swap_chain = factory.CreateSwapChainForHwnd(&device, hwnd, &desc, None, None)?;
            let back: ID3D11Texture2D = swap_chain.GetBuffer(0)?;
            let mut target = None;
            device.CreateRenderTargetView(&back, None, Some(&mut target))?;

            let vs = compile("vs_main", s!("vs_5_0"))?;
            let ps = compile("ps_main", s!("ps_5_0"))?;
            let mut vertex_shader = None;
            device.CreateVertexShader(blob_bytes(&vs), None, Some(&mut vertex_shader))?;
            let mut pixel_shader = None;
            device.CreatePixelShader(blob_bytes(&ps), None, Some(&mut pixel_shader))?;
            let sampler_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_POINT, // pixel for pixel
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                MaxLOD: f32::MAX,
                ..Default::default()
            };
            let mut sampler = None;
            device.CreateSamplerState(&sampler_desc, Some(&mut sampler))?;
            Ok(Self {
                device,
                context,
                swap_chain,
                target: target.ok_or_else(missing)?,
                vertex_shader: vertex_shader.ok_or_else(missing)?,
                pixel_shader: pixel_shader.ok_or_else(missing)?,
                sampler: sampler.ok_or_else(missing)?,
                picture: None,
                size: (width.max(1), height.max(1)),
            })
        }
    }

    /// The device, for the interface's GPU (`UiGpu::new`).
    pub fn device(&self) -> &ID3D11Device {
        &self.device
    }

    /// Sets the picture to show (the frozen screen), uploaded once.
    pub fn set_picture(&mut self, image: &Image) -> Result<()> {
        if !image.is_valid() || image.width == 0 || image.height == 0 {
            return Err(Error::new(E_FAIL, "the picture is empty or damaged"));
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: image.width,
            Height: image.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        let data = D3D11_SUBRESOURCE_DATA {
            pSysMem: image.bgra.as_ptr().cast::<c_void>(),
            SysMemPitch: image.width * 4,
            SysMemSlicePitch: 0,
        };
        // SAFETY: `data` points to width * height * 4 bytes, as validated above,
        // and is copied by the call.
        unsafe {
            let mut texture = None;
            self.device
                .CreateTexture2D(&desc, Some(&data), Some(&mut texture))?;
            let texture = texture.ok_or_else(missing)?;
            let mut view = None;
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut view))?;
            self.picture = view;
        }
        Ok(())
    }

    /// Draws the picture, then lets `chrome` draw on top, and presents.
    pub fn draw(&mut self, chrome: &mut Chrome<'_>) -> Result<()> {
        // SAFETY: every resource is owned by `self` and alive for the draw.
        unsafe {
            self.context
                .OMSetRenderTargets(Some(&[Some(self.target.clone())]), None);
            self.context
                .ClearRenderTargetView(&self.target, &[0.0, 0.0, 0.0, 1.0]);
            self.context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.size.0 as f32,
                Height: self.size.1 as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            if let Some(picture) = &self.picture {
                self.context
                    .IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                self.context.IASetInputLayout(None);
                self.context.VSSetShader(&self.vertex_shader, None);
                self.context.PSSetShader(&self.pixel_shader, None);
                self.context
                    .PSSetShaderResources(0, Some(&[Some(picture.clone())]));
                self.context
                    .PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
                self.context.Draw(3, 0);
            }
            chrome(&self.device, &self.context, self.size);
            self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()
        }
    }
}
