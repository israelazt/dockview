//! A still picture: what a screenshot is made of.

use crate::desktop::PxRect;

/// A picture of 8-bit B, G, R, A pixels, top row first, nothing between rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Image {
    /// An image of one colour (B, G, R, A).
    pub fn solid(width: u32, height: u32, bgra: [u8; 4]) -> Self {
        let mut data = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width as usize * height as usize {
            data.extend_from_slice(&bgra);
        }
        Self {
            width,
            height,
            bgra: data,
        }
    }

    /// Whether the pixel data is as long as the size says.
    pub fn is_valid(&self) -> bool {
        self.bgra.len() == self.width as usize * self.height as usize * 4
    }

    /// The pixel at `(x, y)` as B, G, R, A.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (y as usize * self.width as usize + x as usize) * 4;
        self.bgra.get(at..at + 4)?.try_into().ok()
    }

    /// The part of the picture inside `rect` (in the picture's own pixels);
    /// what falls outside is left out. `None` when nothing is inside.
    pub fn crop(&self, rect: &PxRect) -> Option<Image> {
        let whole = PxRect::new(0, 0, self.width, self.height);
        let r = whole.intersect(rect)?;
        let mut data = Vec::with_capacity(r.w as usize * r.h as usize * 4);
        for row in r.y..r.bottom() {
            let start = (row as usize * self.width as usize + r.x as usize) * 4;
            data.extend_from_slice(self.bgra.get(start..start + r.w as usize * 4)?);
        }
        Some(Image {
            width: r.w,
            height: r.h,
            bgra: data,
        })
    }

    /// The same pixels as R, G, B, A (what PNG and most libraries expect).
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = self.bgra.clone();
        for px in out.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        out
    }

    /// Whether every pixel is black: a capture that failed (a protected window
    /// or a lost device) comes out like this.
    pub fn is_all_black(&self) -> bool {
        self.bgra
            .as_chunks::<4>()
            .0
            .iter()
            .all(|px| px[0] == 0 && px[1] == 0 && px[2] == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3×2 picture whose pixel at (x, y) has blue = x and green = y.
    fn grid() -> Image {
        let mut bgra = Vec::new();
        for y in 0..2u8 {
            for x in 0..3u8 {
                bgra.extend_from_slice(&[x, y, 9, 255]);
            }
        }
        Image {
            width: 3,
            height: 2,
            bgra,
        }
    }

    #[test]
    fn a_solid_image_has_its_colour_everywhere_and_a_valid_size() {
        let i = Image::solid(4, 3, [1, 2, 3, 255]);
        assert!(i.is_valid());
        assert_eq!(i.pixel(3, 2), Some([1, 2, 3, 255]));
        assert_eq!(i.pixel(4, 0), None);
        assert!(!Image {
            width: 2,
            height: 2,
            bgra: vec![0; 5]
        }
        .is_valid());
    }

    #[test]
    fn cropping_keeps_the_pixels_inside_and_cuts_what_is_outside() {
        let c = grid().crop(&PxRect::new(1, 0, 2, 2)).unwrap();
        assert_eq!((c.width, c.height), (2, 2));
        assert_eq!(c.pixel(0, 0), Some([1, 0, 9, 255]));
        assert_eq!(c.pixel(1, 1), Some([2, 1, 9, 255]));
        // Sticking out is cut, entirely outside is nothing.
        let edge = grid().crop(&PxRect::new(2, 1, 5, 5)).unwrap();
        assert_eq!((edge.width, edge.height), (1, 1));
        assert_eq!(edge.pixel(0, 0), Some([2, 1, 9, 255]));
        assert!(grid().crop(&PxRect::new(-5, -5, 2, 2)).is_none());
        assert!(grid().crop(&PxRect::new(0, 0, 0, 3)).is_none());
    }

    #[test]
    fn the_channels_swap_for_rgba_and_black_is_noticed() {
        let i = Image::solid(1, 1, [10, 20, 30, 255]);
        assert_eq!(i.to_rgba(), [30, 20, 10, 255]);
        assert!(Image::solid(2, 2, [0, 0, 0, 255]).is_all_black());
        assert!(!i.is_all_black());
    }
}
