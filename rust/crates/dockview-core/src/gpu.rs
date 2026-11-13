//! What the screen capture and the scene compositor both need from Direct3D 11:
//! a video-capable device, NV12 textures, and a way to read an NV12 picture
//! back into a [`Frame`]. Kept here so each has it once.

use std::time::Instant;

use windows::core::Interface;
use windows::Win32::Foundation::{E_FAIL, HMODULE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, D3D11_VIDEO_COLOR_YCbCrA, ID3D11Device, ID3D11DeviceContext,
    ID3D11Multithread, ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice,
    ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorOutputView,
    D3D11_BIND_RENDER_TARGET, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
    D3D11_VIDEO_COLOR, D3D11_VIDEO_COLOR_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VPOV_DIMENSION_TEXTURE2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};

pub use crate::geometry::even;

use crate::frame::{Frame, FrameSlot, PixelFormat};

/// A Windows rectangle from an origin and a size.
pub fn rect(x: u32, y: u32, w: u32, h: u32) -> RECT {
    RECT {
        left: x as i32,
        top: y as i32,
        right: (x + w) as i32,
        bottom: (y + h) as i32,
    }
}

/// A plain failure with no more detail.
pub fn failed() -> windows::core::Error {
    windows::core::Error::from(E_FAIL)
}

/// A hardware device with BGRA and video-processor support, safe to use from
/// several threads (captures deliver frames on threads of their own).
pub fn create_device() -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    // SAFETY: standard device creation with valid out-pointers, default adapter.
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
    Ok((device, context))
}

/// An NV12 texture: a render target the video processor draws into, or, with
/// `staging`, a copy the CPU can read.
pub fn nv12_texture(
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
    texture.ok_or_else(failed)
}

/// Where a video processor draws its result, and the way back to the CPU: an
/// NV12 render target, its output view and a staging copy to read.
pub struct Nv12Target {
    context: ID3D11DeviceContext,
    texture: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    view: ID3D11VideoProcessorOutputView,
    width: u32,
    height: u32,
}

impl Nv12Target {
    pub fn new(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        video_device: &ID3D11VideoDevice,
        enumerator: &ID3D11VideoProcessorEnumerator,
        width: u32,
        height: u32,
    ) -> windows::core::Result<Self> {
        let texture = nv12_texture(device, width, height, false)?;
        let staging = nv12_texture(device, width, height, true)?;
        let view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        let mut view = None;
        // SAFETY: valid texture, enumerator and description of this device.
        unsafe {
            video_device.CreateVideoProcessorOutputView(
                &texture,
                enumerator,
                &view_desc,
                Some(&mut view),
            )?;
        }
        Ok(Self {
            context: context.clone(),
            texture,
            staging,
            view: view.ok_or_else(failed)?,
            width,
            height,
        })
    }

    /// The view to hand to `VideoProcessorBlt`.
    pub fn view(&self) -> &ID3D11VideoProcessorOutputView {
        &self.view
    }

    /// Makes the whole picture the target and black (in video levels, so
    /// letterbox bars are black and not grey) the background.
    pub fn prepare(&self, video_context: &ID3D11VideoContext, processor: &ID3D11VideoProcessor) {
        let full = rect(0, 0, self.width, self.height);
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
        // SAFETY: plain setters on a processor of the same device.
        unsafe {
            video_context.VideoProcessorSetOutputTargetRect(processor, true, Some(&full));
            video_context.VideoProcessorSetOutputBackgroundColor(processor, true, &black);
        }
    }

    /// Copies the picture to the CPU and publishes it to `slot`.
    ///
    /// Must be called from the thread that drew into it, after the draw.
    pub fn publish(&self, slot: &FrameSlot, timestamp: i64) -> windows::core::Result<()> {
        let (w, h) = (self.width as usize, self.height as usize);
        let expected = PixelFormat::Nv12.buffer_len(w, h);
        // SAFETY: copy and map of textures of this device; the mapped memory is
        // read only until `Unmap`, and every row read is inside the mapping.
        unsafe {
            self.context.CopyResource(&self.staging, &self.texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let mut frame = slot
                .take_spare()
                .filter(|f| {
                    f.format == PixelFormat::Nv12
                        && f.width == self.width
                        && f.height == self.height
                        && f.data.len() == expected
                })
                .unwrap_or_else(|| Frame {
                    width: self.width,
                    height: self.height,
                    stride: self.width,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_is_an_origin_and_a_size() {
        let r = rect(10, 20, 30, 40);
        assert_eq!((r.left, r.top, r.right, r.bottom), (10, 20, 40, 60));
    }

    #[test]
    fn a_target_publishes_a_picture_of_its_size() {
        let Ok((device, context)) = create_device() else {
            eprintln!("no video-capable GPU; skipping");
            return;
        };
        let (Ok(video_device), Ok(video_context)) = (
            device.cast::<ID3D11VideoDevice>(),
            context.cast::<ID3D11VideoContext>(),
        ) else {
            return;
        };
        let content = windows::Win32::Graphics::Direct3D11::D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat:
                windows::Win32::Graphics::Direct3D11::D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputWidth: 320,
            InputHeight: 180,
            OutputWidth: 320,
            OutputHeight: 180,
            Usage: windows::Win32::Graphics::Direct3D11::D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
            ..Default::default()
        };
        // SAFETY: documented setup with valid descriptions.
        let (enumerator, processor) = unsafe {
            let enumerator = video_device
                .CreateVideoProcessorEnumerator(&content)
                .expect("enumerator");
            let processor = video_device
                .CreateVideoProcessor(&enumerator, 0)
                .expect("processor");
            (enumerator, processor)
        };
        let target = Nv12Target::new(&device, &context, &video_device, &enumerator, 320, 180)
            .expect("target");
        target.prepare(&video_context, &processor);
        let slot = FrameSlot::new();
        target.publish(&slot, 42).expect("publish");
        let frame = slot.take_latest().expect("a picture");
        assert_eq!((frame.width, frame.height), (320, 180));
        assert_eq!(frame.format, PixelFormat::Nv12);
        assert_eq!(frame.data.len(), PixelFormat::Nv12.buffer_len(320, 180));
        assert_eq!(frame.timestamp, 42);
    }
}
