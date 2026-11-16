//! Drawing a frame: the video, then the interface on top, then presenting.

use super::*;
use dockview_domain::frame::{ColorMatrix, ColorRange};

/// What the pixel shader reads (`Params` in `render.hlsl`): two rows of four
/// floats. The matrix is decided here, so the shader only follows it.
pub(super) fn shader_params(
    uv_scale_x: f32,
    visible_w: f32,
    height: u32,
    color: ColorInfo,
) -> [f32; 8] {
    let bt709 = color.matrix.resolve(height) == ColorMatrix::Bt709;
    [
        uv_scale_x,
        1.0,
        f32::from(bt709),
        visible_w,
        f32::from(color.range == ColorRange::Full),
        0.0,
        0.0,
        0.0,
    ]
}

impl Renderer {
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
                let (x, y, w, h) = match self.video_area {
                    Some(area) => letterbox_in(area, (video.width, video.height)),
                    None => letterbox(self.size, (video.width, video.height)),
                };
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
                let params = shader_params(uv_scale_x, visible_w, video.height, video.color);
                self.context
                    .UpdateSubresource(&self.params, 0, None, params.as_ptr().cast(), 0, 0);

                let (shader, sampler) = match video.format {
                    PixelFormat::Nv12 if self.filter == Filter::Bicubic => {
                        (&self.ps_nv12_bicubic, &self.sampler_linear)
                    }
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
    pub(super) fn read_back(&self) -> Result<Screenshot> {
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

    pub(super) fn sampler_for_filter(&self) -> &ID3D11SamplerState {
        match self.filter {
            Filter::Nearest => &self.sampler_point,
            Filter::Linear | Filter::Bicubic => &self.sampler_linear,
        }
    }

    pub(super) fn create_target(&mut self) -> Result<()> {
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

    pub(super) fn create_video_textures(&self, frame: &Frame) -> Result<VideoTextures> {
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
            color: frame.color,
            width: frame.width,
            height: frame.height,
            stride: frame.stride,
            primary,
            primary_tex,
            chroma,
        })
    }

    pub(super) fn create_texture(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn params(height: u32, color: ColorInfo) -> [f32; 8] {
        shader_params(1.0, 0.0, height, color)
    }

    #[test]
    fn a_device_that_says_nothing_gets_the_usual_defaults() {
        let unstated = ColorInfo::default();
        assert_eq!(params(1080, unstated)[2], 1.0, "HD is BT.709");
        assert_eq!(params(480, unstated)[2], 0.0, "SD is BT.601");
        assert_eq!(params(1080, unstated)[4], 0.0, "video levels");
    }

    #[test]
    fn what_the_device_states_wins_over_the_height() {
        let bt601_hd = ColorInfo {
            matrix: ColorMatrix::Bt601,
            range: ColorRange::Full,
        };
        let p = params(1080, bt601_hd);
        assert_eq!((p[2], p[4]), (0.0, 1.0));
    }

    #[test]
    fn the_buffer_is_two_full_rows() {
        assert_eq!(std::mem::size_of::<[f32; 8]>(), 32);
    }
}
