//! Sizes and rectangles, as plain arithmetic: how big a picture is made, where
//! it goes inside another, and which part of a source a layer shows. No Windows
//! and no GPU, so all of it is tested directly.

use crate::scene::{Fit, NormRect};

/// The nearest even number not above `n` (NV12 needs even sizes).
pub fn even(n: u32) -> u32 {
    n & !1
}

/// Output size for a source of `width`×`height`: scaled down to at most
/// `max_height` (never up), keeping the shape, and both sides even.
pub fn output_size(width: u32, height: u32, max_height: u32) -> (u32, u32) {
    let (width, height) = (width.max(2), height.max(2));
    if height <= max_height {
        return (even(width).max(2), even(height).max(2));
    }
    let scale = f64::from(max_height) / f64::from(height);
    let w = (f64::from(width) * scale).round() as u32;
    (even(w).max(2), even(max_height).max(2))
}

/// Where a `content_w`×`content_h` picture goes inside an `out_w`×`out_h`
/// canvas: as large as fits, centred, with even coordinates. `(x, y, w, h)`.
pub fn fit_in(content_w: u32, content_h: u32, out_w: u32, out_h: u32) -> (u32, u32, u32, u32) {
    let (content_w, content_h) = (content_w.max(1), content_h.max(1));
    let scale =
        (f64::from(out_w) / f64::from(content_w)).min(f64::from(out_h) / f64::from(content_h));
    let w = even((f64::from(content_w) * scale).round() as u32).clamp(2, out_w);
    let h = even((f64::from(content_h) * scale).round() as u32).clamp(2, out_h);
    (even((out_w - w) / 2), even((out_h - h) / 2), w, h)
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

#[cfg(test)]
mod tests {
    use super::*;

    const OUT: (u32, u32) = (1920, 1080);

    #[test]
    fn sizes_are_made_even() {
        assert_eq!(even(1920), 1920);
        assert_eq!(even(1081), 1080);
        assert_eq!(even(3), 2);
    }

    #[test]
    fn output_size_scales_down_but_never_up_and_stays_even() {
        assert_eq!(output_size(1920, 1080, 1080), (1920, 1080));
        assert_eq!(output_size(1280, 720, 1080), (1280, 720), "never scaled up");
        assert_eq!(output_size(2560, 1440, 1080), (1920, 1080));
        assert_eq!(output_size(3840, 2160, 1080), (1920, 1080));
        // Odd window sizes are rounded down to even.
        assert_eq!(output_size(801, 601, 1080), (800, 600));
        // A tall window is limited by its height.
        let (w, h) = output_size(1000, 2000, 1080);
        assert_eq!(h, 1080);
        assert!(w % 2 == 0 && (w as f64 - 540.0).abs() <= 2.0, "{w}");
        // Degenerate input still gives a usable size.
        assert_eq!(output_size(0, 0, 1080), (2, 2));
    }

    #[test]
    fn a_picture_is_fitted_and_centred_with_black_bars() {
        // Same shape: fills the canvas.
        assert_eq!(fit_in(1920, 1080, 1920, 1080), (0, 0, 1920, 1080));
        // A 4:3 picture in a 16:9 canvas: bars left and right.
        let (x, y, w, h) = fit_in(1600, 1200, 1920, 1080);
        assert_eq!((y, h), (0, 1080));
        assert_eq!(w, 1440);
        assert_eq!(x, 240);
        // A window that grew: rescaled into the same canvas.
        let (x, y, w, h) = fit_in(3000, 1000, 1920, 1080);
        assert_eq!((x, w), (0, 1920));
        assert!(y > 0 && h < 1080 && h % 2 == 0 && y % 2 == 0);
        // Everything stays inside the canvas and even.
        for (cw, ch) in [(1, 1), (7, 3), (5000, 100), (100, 5000)] {
            let (x, y, w, h) = fit_in(cw, ch, 1280, 720);
            assert!(x + w <= 1280 && y + h <= 720, "{cw}x{ch}");
            assert!(
                x % 2 == 0 && y % 2 == 0 && w % 2 == 0 && h % 2 == 0,
                "{cw}x{ch}"
            );
        }
    }

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
}
