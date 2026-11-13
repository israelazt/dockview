//! Clipping of the interface quads to a rectangle, on the CPU.
//!
//! The interface is drawn as axis-aligned quads (rounded shapes and glyphs are
//! both one quad each, with the shape or the texture coordinates worked out
//! per pixel on the GPU). Everything the quad carries (`uv`) is a straight
//! function of the position inside it, so cutting a quad down to a rectangle
//! is just moving its corners and working out the new `uv` at each.

use dockview_domain::draw::Vertex;

/// A clip rectangle in physical pixels: `[left, top, right, bottom]`.
pub type ClipRect = [f32; 4];

/// The part of the quad `q` (top-left, top-right, bottom-right, bottom-left)
/// that lies inside `clip`; `None` when nothing does.
pub fn clip_quad(q: &[Vertex; 4], clip: ClipRect) -> Option<[Vertex; 4]> {
    let (tl, br) = (q[0], q[2]);
    let left = tl.pos[0].max(clip[0]);
    let top = tl.pos[1].max(clip[1]);
    let right = br.pos[0].min(clip[2]);
    let bottom = br.pos[1].min(clip[3]);
    if left >= right || top >= bottom {
        return None;
    }
    let (width, height) = (br.pos[0] - tl.pos[0], br.pos[1] - tl.pos[1]);
    // A quad that is not cut is returned as it is.
    if left == tl.pos[0] && top == tl.pos[1] && right == br.pos[0] && bottom == br.pos[1] {
        return Some(*q);
    }
    let at = |x: f32, y: f32| {
        let (tx, ty) = (
            if width > 0.0 {
                (x - tl.pos[0]) / width
            } else {
                0.0
            },
            if height > 0.0 {
                (y - tl.pos[1]) / height
            } else {
                0.0
            },
        );
        Vertex {
            pos: [x, y],
            uv: [
                tl.uv[0] + (br.uv[0] - tl.uv[0]) * tx,
                tl.uv[1] + (br.uv[1] - tl.uv[1]) * ty,
            ],
            color: tl.color,
            shape: tl.shape,
        }
    };
    Some([
        at(left, top),
        at(right, top),
        at(right, bottom),
        at(left, bottom),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A quad of `w` x `h` at `(x, y)` whose `uv` run from (0, 0) to (1, 1).
    fn quad(x: f32, y: f32, w: f32, h: f32) -> [Vertex; 4] {
        let v = |px: f32, py: f32, u: f32, vv: f32| Vertex {
            pos: [px, py],
            uv: [u, vv],
            color: [1.0, 0.5, 0.25, 1.0],
            shape: [3.0, 4.0, 5.0, 6.0],
        };
        [
            v(x, y, 0.0, 0.0),
            v(x + w, y, 1.0, 0.0),
            v(x + w, y + h, 1.0, 1.0),
            v(x, y + h, 0.0, 1.0),
        ]
    }

    #[test]
    fn a_quad_inside_the_clip_is_untouched() {
        let q = quad(10.0, 10.0, 20.0, 20.0);
        assert_eq!(clip_quad(&q, [0.0, 0.0, 100.0, 100.0]), Some(q));
    }

    #[test]
    fn a_quad_outside_the_clip_disappears() {
        let q = quad(10.0, 10.0, 20.0, 20.0);
        assert_eq!(clip_quad(&q, [50.0, 0.0, 100.0, 100.0]), None);
        assert_eq!(clip_quad(&q, [0.0, 50.0, 100.0, 100.0]), None);
        // Touching an edge is not overlapping.
        assert_eq!(clip_quad(&q, [30.0, 0.0, 100.0, 100.0]), None);
    }

    #[test]
    fn a_cut_quad_keeps_its_corners_inside_and_its_texture_in_step() {
        // The right half is cut off: the texture must stop half way too.
        let q = quad(0.0, 0.0, 20.0, 10.0);
        let cut = clip_quad(&q, [0.0, 0.0, 10.0, 10.0]).unwrap();
        assert_eq!(cut[0].pos, [0.0, 0.0]);
        assert_eq!(cut[2].pos, [10.0, 10.0]);
        assert_eq!(cut[1].uv, [0.5, 0.0]);
        assert_eq!(cut[2].uv, [0.5, 1.0]);
        assert_eq!(cut[3].uv, [0.0, 1.0]);
    }

    #[test]
    fn cutting_the_top_moves_the_texture_down_with_it() {
        let q = quad(0.0, 0.0, 10.0, 40.0);
        let cut = clip_quad(&q, [0.0, 10.0, 10.0, 40.0]).unwrap();
        assert_eq!(cut[0].pos, [0.0, 10.0]);
        assert_eq!(cut[0].uv, [0.0, 0.25]);
        assert_eq!(cut[2].uv, [1.0, 1.0]);
    }

    #[test]
    fn colour_and_shape_are_not_changed_by_a_cut() {
        let q = quad(0.0, 0.0, 20.0, 20.0);
        let cut = clip_quad(&q, [5.0, 5.0, 15.0, 15.0]).unwrap();
        for v in cut {
            assert_eq!(v.color, [1.0, 0.5, 0.25, 1.0]);
            assert_eq!(v.shape, [3.0, 4.0, 5.0, 6.0]);
        }
    }

    #[test]
    fn a_rounded_shape_keeps_its_position_relative_to_its_centre() {
        // For these quads `uv` is the offset from the centre in pixels, which
        // is a straight function of the position: it must stay that way.
        let (x, y, w, h) = (100.0, 50.0, 40.0, 20.0);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let v = |px: f32, py: f32| Vertex {
            pos: [px, py],
            uv: [px - cx, py - cy],
            color: [1.0; 4],
            shape: [w / 2.0, h / 2.0, 4.0, 0.0],
        };
        let q = [v(x, y), v(x + w, y), v(x + w, y + h), v(x, y + h)];
        let cut = clip_quad(&q, [110.0, 55.0, 130.0, 65.0]).unwrap();
        for corner in cut {
            assert_eq!(corner.uv, [corner.pos[0] - cx, corner.pos[1] - cy]);
        }
    }
}
