//! A video frame as a still picture. The colour arithmetic is the same as the
//! renderer's shader (`render.hlsl`), so the picture matches what is on screen.

use crate::frame::{ColorMatrix, ColorRange, Frame, PixelFormat};
use crate::image::Image;

/// The numbers of one frame's YUV to RGB conversion, ready to use per pixel.
struct Conversion {
    full_range: bool,
    /// How much of V goes to red, of U and V to green and of U to blue.
    red_v: f32,
    green_u: f32,
    green_v: f32,
    blue_u: f32,
}

impl Conversion {
    fn of(frame: &Frame) -> Self {
        let hd = frame.color.matrix.resolve(frame.height) == ColorMatrix::Bt709;
        let (red_v, green_u, green_v, blue_u) = if hd {
            (1.5748, 0.1873, 0.4681, 1.8556)
        } else {
            (1.402, 0.3441, 0.7141, 1.772)
        };
        Self {
            full_range: frame.color.range == ColorRange::Full,
            red_v,
            green_u,
            green_v,
            blue_u,
        }
    }

    fn bgra(&self, y: u8, u: u8, v: u8) -> [u8; 4] {
        let mut y = f32::from(y);
        let (mut u, mut v) = (f32::from(u) - 128.0, f32::from(v) - 128.0);
        if !self.full_range {
            y = (y - 16.0) * (255.0 / 219.0);
            u *= 255.0 / 224.0;
            v *= 255.0 / 224.0;
        }
        let byte = |x: f32| x.round().clamp(0.0, 255.0) as u8;
        [
            byte(y + self.blue_u * u),
            byte(y - self.green_u * u - self.green_v * v),
            byte(y + self.red_v * v),
            255,
        ]
    }
}

/// The picture in `frame`, at its full size. `None` for a frame with no
/// pixels, an odd width or less data than its size says.
pub fn image_of(frame: &Frame) -> Option<Image> {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let stride = frame.stride as usize;
    let row_bytes = match frame.format {
        PixelFormat::Nv12 => width,
        PixelFormat::Yuy2 => width * 2,
    };
    let odd = width % 2 != 0 || (frame.format == PixelFormat::Nv12 && height % 2 != 0);
    if width == 0
        || height == 0
        || odd
        || stride < row_bytes
        || frame.data.len() < frame.format.buffer_len(stride, height)
    {
        return None;
    }
    let convert = Conversion::of(frame);
    let data = &frame.data;
    let mut bgra = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        for x in (0..width).step_by(2) {
            // Two pixels side by side share one U and one V.
            let (y0, y1, u, v) = match frame.format {
                PixelFormat::Nv12 => {
                    let luma = row * stride + x;
                    let chroma = (height + row / 2) * stride + x;
                    (data[luma], data[luma + 1], data[chroma], data[chroma + 1])
                }
                PixelFormat::Yuy2 => {
                    let at = row * stride + x * 2;
                    (data[at], data[at + 2], data[at + 1], data[at + 3])
                }
            };
            bgra.extend_from_slice(&convert.bgra(y0, u, v));
            bgra.extend_from_slice(&convert.bgra(y1, u, v));
        }
    }
    Some(Image {
        width: frame.width,
        height: frame.height,
        bgra,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::frame::ColorInfo;

    fn frame(format: PixelFormat, size: (u32, u32), stride: u32, data: Vec<u8>) -> Frame {
        Frame {
            width: size.0,
            height: size.1,
            stride,
            format,
            color: ColorInfo::default(),
            data,
            timestamp: 0,
            arrived: Instant::now(),
        }
    }

    #[test]
    fn video_levels_stretch_to_black_and_white() {
        // 2x2 NV12 with a padded row: black on the left, white on the right.
        let data = vec![16, 235, 7, 7, 16, 235, 7, 7, 128, 128, 7, 7];
        let image = image_of(&frame(PixelFormat::Nv12, (2, 2), 4, data)).unwrap();
        assert!(image.is_valid());
        assert_eq!(image.pixel(0, 0), Some([0, 0, 0, 255]));
        assert_eq!(image.pixel(1, 1), Some([255, 255, 255, 255]));
    }

    #[test]
    fn full_range_levels_are_left_as_they_are() {
        let mut f = frame(PixelFormat::Yuy2, (2, 1), 4, vec![16, 128, 235, 128]);
        f.color.range = ColorRange::Full;
        let image = image_of(&f).unwrap();
        assert_eq!(image.pixel(0, 0), Some([16, 16, 16, 255]));
        assert_eq!(image.pixel(1, 0), Some([235, 235, 235, 255]));
    }

    #[test]
    fn red_comes_out_red_in_both_layouts() {
        // Video-range YUV of pure red in BT.601.
        let (y, u, v) = (81, 90, 240);
        for (format, stride, data) in [
            (PixelFormat::Nv12, 2, vec![y, y, y, y, u, v]),
            (PixelFormat::Yuy2, 4, vec![y, u, y, v, y, u, y, v]),
        ] {
            let mut f = frame(format, (2, 2), stride, data);
            f.color.matrix = ColorMatrix::Bt601;
            let [b, g, r, _] = image_of(&f).unwrap().pixel(1, 1).unwrap();
            assert!(r > 250 && g < 5 && b < 5, "{format:?}: {r} {g} {b}");
        }
    }

    #[test]
    fn a_frame_that_does_not_add_up_gives_no_picture() {
        for (size, stride, len) in [
            ((0, 0), 0, 0),
            ((4, 2), 4, 11),
            ((4, 2), 2, 99),
            ((3, 2), 4, 99),
        ] {
            let f = frame(PixelFormat::Nv12, size, stride, vec![0; len]);
            assert!(image_of(&f).is_none(), "{size:?} {stride} {len}");
        }
    }
}
