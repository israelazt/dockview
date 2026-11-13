//! Direct3D 11 side of the interface layer: pipeline state, glyph atlas texture and
//! the dynamic vertex buffer.

use std::mem::size_of;

use windows::core::{s, Error, Result};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11BlendState, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11InputLayout,
    ID3D11PixelShader, ID3D11RasterizerState, ID3D11SamplerState, ID3D11ShaderResourceView,
    ID3D11Texture2D, ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_SHADER_RESOURCE,
    D3D11_BIND_VERTEX_BUFFER, D3D11_BLEND_DESC, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_ONE,
    D3D11_BLEND_OP_ADD, D3D11_BLEND_SRC_ALPHA, D3D11_BOX, D3D11_BUFFER_DESC,
    D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_COMPARISON_NEVER, D3D11_CPU_ACCESS_WRITE, D3D11_CULL_NONE,
    D3D11_FILL_SOLID, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_INPUT_ELEMENT_DESC,
    D3D11_INPUT_PER_VERTEX_DATA, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_WRITE_DISCARD,
    D3D11_RASTERIZER_DESC, D3D11_RENDER_TARGET_BLEND_DESC, D3D11_SAMPLER_DESC,
    D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC,
    D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R8_UNORM,
    DXGI_SAMPLE_DESC,
};

use dockview_domain::draw::{DrawList, Vertex, ATLAS_SIZE};

use crate::render::{blob_bytes, compile};

const SHADER_SOURCE: &str = include_str!("ui.hlsl");
/// Capacity of the vertex buffer, in vertices.
const MAX_VERTICES: usize = 64 * 1024;

fn missing() -> Error {
    Error::new(E_FAIL, "D3D11 returned no object")
}

pub struct UiGpu {
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    vertex_buffer: ID3D11Buffer,
    frame_buffer: ID3D11Buffer,
    atlas: ID3D11Texture2D,
    atlas_view: ID3D11ShaderResourceView,
    sampler: ID3D11SamplerState,
    blend: ID3D11BlendState,
    rasterizer: ID3D11RasterizerState,
}

impl UiGpu {
    pub fn new(device: &ID3D11Device) -> Result<Self> {
        // SAFETY: standard D3D11 resource creation with valid descriptors.
        unsafe {
            let vs_blob = compile(SHADER_SOURCE, "vs_main", s!("vs_5_0"))?;
            let ps_blob = compile(SHADER_SOURCE, "ps_main", s!("ps_5_0"))?;
            let mut vertex_shader = None;
            device.CreateVertexShader(blob_bytes(&vs_blob), None, Some(&mut vertex_shader))?;
            let mut pixel_shader = None;
            device.CreatePixelShader(blob_bytes(&ps_blob), None, Some(&mut pixel_shader))?;

            let element = |name, format, offset| D3D11_INPUT_ELEMENT_DESC {
                SemanticName: name,
                SemanticIndex: 0,
                Format: format,
                InputSlot: 0,
                AlignedByteOffset: offset,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            };
            let mut second = element(s!("TEXCOORD"), DXGI_FORMAT_R32G32B32A32_FLOAT, 32);
            second.SemanticIndex = 1;
            let elements = [
                element(s!("POSITION"), DXGI_FORMAT_R32G32_FLOAT, 0),
                element(s!("TEXCOORD"), DXGI_FORMAT_R32G32_FLOAT, 8),
                element(s!("COLOR"), DXGI_FORMAT_R32G32B32A32_FLOAT, 16),
                second,
            ];
            let mut layout = None;
            device.CreateInputLayout(&elements, blob_bytes(&vs_blob), Some(&mut layout))?;

            let mut vertex_buffer = None;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: (MAX_VERTICES * size_of::<Vertex>()) as u32,
                    Usage: D3D11_USAGE_DYNAMIC,
                    BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                    CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut vertex_buffer),
            )?;

            let mut frame_buffer = None;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 16,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut frame_buffer),
            )?;

            let mut atlas = None;
            device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: ATLAS_SIZE,
                    Height: ATLAS_SIZE,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_R8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut atlas),
            )?;
            let atlas = atlas.ok_or_else(missing)?;
            let mut atlas_view = None;
            device.CreateShaderResourceView(&atlas, None, Some(&mut atlas_view))?;

            let mut sampler = None;
            device.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    ComparisonFunc: D3D11_COMPARISON_NEVER,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                },
                Some(&mut sampler),
            )?;

            let mut blend_desc = D3D11_BLEND_DESC::default();
            blend_desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_SRC_ALPHA,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let mut blend = None;
            device.CreateBlendState(&blend_desc, Some(&mut blend))?;

            let mut rasterizer = None;
            device.CreateRasterizerState(
                &D3D11_RASTERIZER_DESC {
                    FillMode: D3D11_FILL_SOLID,
                    CullMode: D3D11_CULL_NONE,
                    ..Default::default()
                },
                Some(&mut rasterizer),
            )?;

            Ok(Self {
                vertex_shader: vertex_shader.ok_or_else(missing)?,
                pixel_shader: pixel_shader.ok_or_else(missing)?,
                layout: layout.ok_or_else(missing)?,
                vertex_buffer: vertex_buffer.ok_or_else(missing)?,
                frame_buffer: frame_buffer.ok_or_else(missing)?,
                atlas,
                atlas_view: atlas_view.ok_or_else(missing)?,
                sampler: sampler.ok_or_else(missing)?,
                blend: blend.ok_or_else(missing)?,
                rasterizer: rasterizer.ok_or_else(missing)?,
            })
        }
    }

    /// Uploads any new glyphs and draws everything queued in `ui`.
    pub fn render(&mut self, context: &ID3D11DeviceContext, list: &DrawList<'_>) -> Result<()> {
        let target_size = list.target_size;
        // SAFETY: all resources belong to the device of `context`; the vertex
        // data is plain-old-data and copied within the buffer's capacity.
        unsafe {
            for upload in &list.uploads {
                let region = D3D11_BOX {
                    left: upload.x,
                    top: upload.y,
                    front: 0,
                    right: upload.x + upload.w,
                    bottom: upload.y + upload.h,
                    back: 1,
                };
                context.UpdateSubresource(
                    &self.atlas,
                    0,
                    Some(&region),
                    upload.data.as_ptr().cast(),
                    upload.w,
                    0,
                );
            }

            let vertices = list.vertices;
            let count = vertices.len().min(MAX_VERTICES);
            if count == 0 {
                return Ok(());
            }
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(
                &self.vertex_buffer,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                Some(&mut mapped),
            )?;
            std::ptr::copy_nonoverlapping(
                vertices.as_ptr().cast::<u8>(),
                mapped.pData.cast::<u8>(),
                count * size_of::<Vertex>(),
            );
            context.Unmap(&self.vertex_buffer, 0);

            let frame = [target_size.0 as f32, target_size.1 as f32, 0.0, 0.0];
            context.UpdateSubresource(&self.frame_buffer, 0, None, frame.as_ptr().cast(), 0, 0);

            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: target_size.0 as f32,
                Height: target_size.1 as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            context.RSSetState(&self.rasterizer);
            context.OMSetBlendState(&self.blend, None, 0xFFFF_FFFF);
            context.IASetInputLayout(&self.layout);
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            let stride = size_of::<Vertex>() as u32;
            context.IASetVertexBuffers(
                0,
                1,
                Some(&Some(self.vertex_buffer.clone())),
                Some(&stride),
                Some(&0),
            );
            context.VSSetShader(&self.vertex_shader, None);
            context.VSSetConstantBuffers(0, Some(&[Some(self.frame_buffer.clone())]));
            context.PSSetShader(&self.pixel_shader, None);
            context.PSSetShaderResources(0, Some(&[Some(self.atlas_view.clone())]));
            context.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            context.Draw(count as u32, 0);

            // Hand the pipeline back in the state the video pass expects.
            context.OMSetBlendState(None, None, 0xFFFF_FFFF);
            context.RSSetState(None);
            context.IASetInputLayout(None);
            context.IASetVertexBuffers(0, 1, Some(&None), Some(&0), Some(&0));
        }
        Ok(())
    }
}
