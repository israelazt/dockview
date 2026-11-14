//! The geometry of the free layer editor: where the picture is on screen,
//! which handle of a layer the pointer is on, and what moving or resizing does
//! to its rectangle. Pure arithmetic in two spaces: *screen* (logical pixels)
//! and *normalised* (`0..1` of the picture, as scenes store it).

use crate::scene::{NormRect, MIN_LAYER_SIZE};

/// Distance, in pixels, within which an edge or the centre pulls a layer to it.
pub const SNAP_PX: f32 = 6.0;
/// Size, in pixels, of the area that grabs a handle.
pub const GRAB_PX: f32 = 12.0;
/// The unit rectangles travel in through actions (ten-thousandths).
const UNITS: f32 = 10_000.0;

/// Where the picture is on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Picture {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Picture {
    /// The picture of `size` (width, height) as large as fits in `area` (x, y,
    /// width, height), centred: how the renderer draws it.
    pub fn fitted(area: (f32, f32, f32, f32), size: (u32, u32)) -> Self {
        let (vw, vh) = (size.0.max(1) as f32, size.1.max(1) as f32);
        let scale = (area.2 / vw).min(area.3 / vh).max(0.0);
        let (w, h) = (vw * scale, vh * scale);
        Self {
            x: area.0 + (area.2 - w) / 2.0,
            y: area.1 + (area.3 - h) / 2.0,
            w,
            h,
        }
    }

    /// A normalised rectangle on screen: `(x, y, width, height)`.
    pub fn to_screen(&self, r: NormRect) -> (f32, f32, f32, f32) {
        (
            self.x + r.x * self.w,
            self.y + r.y * self.h,
            r.w * self.w,
            r.h * self.h,
        )
    }

    /// A point on screen as a point of the picture (it may lie outside `0..1`).
    pub fn to_norm(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.x) / self.w.max(1.0),
            (y - self.y) / self.h.max(1.0),
        )
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

/// A rectangle as the integers actions carry.
pub fn to_units(r: NormRect) -> [i32; 4] {
    [r.x, r.y, r.w, r.h].map(|v| (v * UNITS).round() as i32)
}

/// The rectangle those integers stand for, pulled back inside the picture.
pub fn from_units(u: [i32; 4]) -> NormRect {
    let [x, y, w, h] = u.map(|v| v as f32 / UNITS);
    NormRect::new(x, y, w, h).clamped()
}

/// What the pointer holds on a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    /// The body: moves the layer.
    Move,
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl Handle {
    /// The eight handles, corners first (they win where handles overlap).
    pub const ALL: [Handle; 8] = [
        Handle::TopLeft,
        Handle::TopRight,
        Handle::BottomRight,
        Handle::BottomLeft,
        Handle::Top,
        Handle::Right,
        Handle::Bottom,
        Handle::Left,
    ];

    /// Which edges it moves: (left, top, right, bottom).
    fn edges(self) -> (bool, bool, bool, bool) {
        match self {
            Handle::Move => (false, false, false, false),
            Handle::TopLeft => (true, true, false, false),
            Handle::Top => (false, true, false, false),
            Handle::TopRight => (false, true, true, false),
            Handle::Right => (false, false, true, false),
            Handle::BottomRight => (false, false, true, true),
            Handle::Bottom => (false, false, false, true),
            Handle::BottomLeft => (true, false, false, true),
            Handle::Left => (true, false, false, false),
        }
    }

    fn is_corner(self) -> bool {
        matches!(
            self,
            Handle::TopLeft | Handle::TopRight | Handle::BottomRight | Handle::BottomLeft
        )
    }

    /// Where the handle sits on `r`, as fractions of the rectangle.
    pub fn anchor(self) -> (f32, f32) {
        let (l, t, r, b) = self.edges();
        let x = if l {
            0.0
        } else if r {
            1.0
        } else {
            0.5
        };
        let y = if t {
            0.0
        } else if b {
            1.0
        } else {
            0.5
        };
        (x, y)
    }
}

/// The handle of `rect` (the selected layer) under the pointer at `(x, y)`: a
/// corner or edge handle first, then the body. `None` when it is on neither.
pub fn handle_at(picture: &Picture, rect: NormRect, x: f32, y: f32) -> Option<Handle> {
    let (rx, ry, rw, rh) = picture.to_screen(rect);
    let near =
        |hx: f32, hy: f32| (x - hx).abs() <= GRAB_PX / 2.0 && (y - hy).abs() <= GRAB_PX / 2.0;
    for handle in Handle::ALL {
        let (ax, ay) = handle.anchor();
        if near(rx + ax * rw, ry + ay * rh) {
            return Some(handle);
        }
    }
    (x >= rx && x <= rx + rw && y >= ry && y <= ry + rh).then_some(Handle::Move)
}

/// The topmost layer under the pointer: `layers` run bottom first, and a
/// hidden one (`false`) cannot be picked.
pub fn pick_layer(picture: &Picture, layers: &[(NormRect, bool)], x: f32, y: f32) -> Option<usize> {
    let (nx, ny) = picture.to_norm(x, y);
    layers
        .iter()
        .enumerate()
        .rev()
        .find(|(_, (rect, visible))| *visible && rect.contains(nx, ny))
        .map(|(i, _)| i)
}

/// A drag in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    pub handle: Handle,
    /// The rectangle when the drag began.
    pub start: NormRect,
    /// Where the pointer was then, in the picture.
    pub origin: (f32, f32),
}

impl Drag {
    pub fn begin(handle: Handle, start: NormRect, picture: &Picture, x: f32, y: f32) -> Self {
        Self {
            handle,
            start,
            origin: picture.to_norm(x, y),
        }
    }
}

/// How a drag is modified.
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    /// Keep the proportion of the rectangle (corner handles).
    pub keep_aspect: bool,
    /// Do not pull to edges and the centre.
    pub no_snap: bool,
}

/// The lines a layer is pulled to along one axis: the picture's edges and
/// centre, and the edges and centres of the other layers.
fn snap_lines(others: &[NormRect], horizontal: bool) -> Vec<f32> {
    let mut lines = vec![0.0, 0.5, 1.0];
    for r in others {
        let (start, len) = if horizontal { (r.x, r.w) } else { (r.y, r.h) };
        lines.extend([start, start + len / 2.0, start + len]);
    }
    lines
}

/// The shift that brings the closest of `points` onto the closest of `lines`,
/// when it is within `threshold`.
fn snap_shift(points: &[f32], lines: &[f32], threshold: f32) -> Option<f32> {
    let mut best: Option<f32> = None;
    for p in points {
        for l in lines {
            let d = l - p;
            if d.abs() <= threshold && best.is_none_or(|b| d.abs() < b.abs()) {
                best = Some(d);
            }
        }
    }
    best
}

/// The rectangle after the pointer moved to `(x, y)`: the drag's start, moved
/// or resized by how far the pointer went, pulled to `others` and the picture
/// unless `mods` says not, never smaller than the minimum or outside the
/// picture.
pub fn drag_to(
    drag: &Drag,
    picture: &Picture,
    (x, y): (f32, f32),
    others: &[NormRect],
    mods: Modifiers,
) -> NormRect {
    let (px, py) = picture.to_norm(x, y);
    let (dx, dy) = (px - drag.origin.0, py - drag.origin.1);
    let s = drag.start;
    let snap_x = (!mods.no_snap).then(|| snap_lines(others, true));
    let snap_y = (!mods.no_snap).then(|| snap_lines(others, false));
    let (tx, ty) = (SNAP_PX / picture.w.max(1.0), SNAP_PX / picture.h.max(1.0));

    if drag.handle == Handle::Move {
        let mut nx = s.x + dx;
        let mut ny = s.y + dy;
        if let Some(lines) = &snap_x {
            if let Some(d) = snap_shift(&[nx, nx + s.w / 2.0, nx + s.w], lines, tx) {
                nx += d;
            }
        }
        if let Some(lines) = &snap_y {
            if let Some(d) = snap_shift(&[ny, ny + s.h / 2.0, ny + s.h], lines, ty) {
                ny += d;
            }
        }
        return NormRect::new(nx.clamp(0.0, 1.0 - s.w), ny.clamp(0.0, 1.0 - s.h), s.w, s.h);
    }

    let (left, top, right, bottom) = drag.handle.edges();
    let (mut l, mut t, mut r, mut b) = (s.x, s.y, s.x + s.w, s.y + s.h);
    if left {
        l += dx;
    }
    if right {
        r += dx;
    }
    if top {
        t += dy;
    }
    if bottom {
        b += dy;
    }
    // Only a moving edge is pulled; the aspect lock below decides the rest.
    let pull = |edge: &mut f32, lines: &Option<Vec<f32>>, threshold: f32| {
        if let Some(lines) = lines {
            if let Some(d) = snap_shift(&[*edge], lines, threshold) {
                *edge += d;
            }
        }
    };
    if !(mods.keep_aspect && drag.handle.is_corner()) {
        if left {
            pull(&mut l, &snap_x, tx);
        }
        if right {
            pull(&mut r, &snap_x, tx);
        }
        if top {
            pull(&mut t, &snap_y, ty);
        }
        if bottom {
            pull(&mut b, &snap_y, ty);
        }
    }
    l = l.clamp(0.0, 1.0);
    r = r.clamp(0.0, 1.0);
    t = t.clamp(0.0, 1.0);
    b = b.clamp(0.0, 1.0);

    if mods.keep_aspect && drag.handle.is_corner() {
        return resize_keeping_aspect(drag, (l, t, r, b));
    }
    // The edge being dragged stops at the minimum size, never crossing the
    // opposite one.
    if left {
        l = l.min(r - MIN_LAYER_SIZE);
    } else if right {
        r = r.max(l + MIN_LAYER_SIZE);
    }
    if top {
        t = t.min(b - MIN_LAYER_SIZE);
    } else if bottom {
        b = b.max(t + MIN_LAYER_SIZE);
    }
    NormRect::new(l, t, r - l, b - t).clamped()
}

/// A corner drag that keeps the proportion: the opposite corner stays, and the
/// size follows the pointer along the wider of the two directions.
fn resize_keeping_aspect(drag: &Drag, (l, t, r, b): (f32, f32, f32, f32)) -> NormRect {
    let s = drag.start;
    let ratio = (s.w / s.h.max(f32::EPSILON)).max(f32::EPSILON);
    let (left, top, _, _) = drag.handle.edges();
    // The corner that stays put, and the room towards the dragged one.
    let (ax, ay) = (
        if left { s.x + s.w } else { s.x },
        if top { s.y + s.h } else { s.y },
    );
    let (room_w, room_h) = (
        if left { ax } else { 1.0 - ax },
        if top { ay } else { 1.0 - ay },
    );
    let wanted_w = if left { ax - l } else { r - ax };
    let wanted_h = if top { ay - t } else { b - ay };
    let mut w = wanted_w.max(wanted_h * ratio);
    w = w.min(room_w).min(room_h * ratio);
    w = w.max(MIN_LAYER_SIZE).max(MIN_LAYER_SIZE * ratio);
    let h = w / ratio;
    let x = if left { ax - w } else { ax };
    let y = if top { ay - h } else { ay };
    NormRect::new(x, y, w, h).clamped()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1000×500 picture at (100, 50).
    fn picture() -> Picture {
        Picture {
            x: 100.0,
            y: 50.0,
            w: 1000.0,
            h: 500.0,
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> NormRect {
        NormRect::new(x, y, w, h)
    }

    fn close(a: NormRect, b: NormRect) {
        for (p, q) in [(a.x, b.x), (a.y, b.y), (a.w, b.w), (a.h, b.h)] {
            assert!((p - q).abs() < 1e-4, "{a:?} != {b:?}");
        }
    }

    fn drag(handle: Handle, start: NormRect, to: (f32, f32), mods: Modifiers) -> NormRect {
        // The drag begins on the handle's own spot.
        let p = picture();
        let (ax, ay) = handle.anchor();
        let (sx, sy, sw, sh) = p.to_screen(start);
        let d = Drag::begin(handle, start, &p, sx + ax * sw, sy + ay * sh);
        drag_to(&d, &p, to, &[], mods)
    }

    const NO_SNAP: Modifiers = Modifiers {
        keep_aspect: false,
        no_snap: true,
    };

    #[test]
    fn the_picture_fits_its_area_centred_like_the_renderer_does() {
        let p = Picture::fitted((0.0, 0.0, 1000.0, 1000.0), (1920, 1080));
        assert!((p.w - 1000.0).abs() < 1e-2 && (p.h - 562.5).abs() < 1e-2);
        assert!(p.x.abs() < 1e-3 && (p.y - 218.75).abs() < 1e-2);
        let tall = Picture::fitted((10.0, 20.0, 500.0, 600.0), (1000, 500));
        assert_eq!(
            (tall.w, tall.h, tall.x, tall.y),
            (500.0, 250.0, 10.0, 195.0)
        );
        // An empty area does not divide by zero.
        let none = Picture::fitted((0.0, 0.0, 0.0, 0.0), (0, 0));
        assert_eq!((none.w, none.h), (0.0, 0.0));
    }

    #[test]
    fn screen_and_normalised_coordinates_convert_both_ways() {
        let p = picture();
        assert_eq!(
            p.to_screen(rect(0.5, 0.5, 0.25, 0.5)),
            (600.0, 300.0, 250.0, 250.0)
        );
        assert_eq!(p.to_norm(600.0, 300.0), (0.5, 0.5));
        assert!(p.contains(100.0, 50.0) && !p.contains(99.0, 50.0));
    }

    #[test]
    fn rectangles_survive_the_integer_units() {
        let r = rect(0.123456, 0.5, 0.25, 0.3333);
        let back = from_units(to_units(r));
        assert!((back.x - 0.1235).abs() < 1e-4 && (back.h - 0.3333).abs() < 1e-4);
        // Nonsense is pulled inside the picture.
        let wild = from_units([-5000, 20000, 90000, 1]);
        assert!(wild.x >= 0.0 && wild.x + wild.w <= 1.0 + 1e-6);
        assert!(wild.h >= MIN_LAYER_SIZE);
    }

    #[test]
    fn the_handles_and_the_body_are_found_where_they_are_drawn() {
        let p = picture();
        let r = rect(0.2, 0.2, 0.4, 0.4); // screen: x 300..700, y 150..350
        assert_eq!(handle_at(&p, r, 300.0, 150.0), Some(Handle::TopLeft));
        assert_eq!(handle_at(&p, r, 700.0, 350.0), Some(Handle::BottomRight));
        assert_eq!(handle_at(&p, r, 500.0, 150.0), Some(Handle::Top));
        assert_eq!(handle_at(&p, r, 300.0, 250.0), Some(Handle::Left));
        assert_eq!(handle_at(&p, r, 500.0, 250.0), Some(Handle::Move));
        assert_eq!(handle_at(&p, r, 900.0, 250.0), None);
        // A little off the corner still grabs it.
        assert_eq!(handle_at(&p, r, 304.0, 154.0), Some(Handle::TopLeft));
    }

    #[test]
    fn the_topmost_visible_layer_is_picked() {
        let p = picture();
        let layers = [
            (NormRect::FULL, true),
            (rect(0.5, 0.5, 0.4, 0.4), true),
            (rect(0.5, 0.5, 0.4, 0.4), false),
        ];
        assert_eq!(
            pick_layer(&p, &layers, 800.0, 400.0),
            Some(1),
            "the hidden one is skipped"
        );
        assert_eq!(pick_layer(&p, &layers, 200.0, 100.0), Some(0));
        assert_eq!(
            pick_layer(&p, &layers, 50.0, 100.0),
            None,
            "outside the picture"
        );
    }

    #[test]
    fn moving_follows_the_pointer_and_stays_inside_the_picture() {
        let start = rect(0.1, 0.1, 0.2, 0.2);
        // 100 px right and 50 px down of a 1000×500 picture is 0.1 and 0.1.
        let p = picture();
        let d = Drag::begin(Handle::Move, start, &p, 300.0, 150.0);
        close(
            drag_to(&d, &p, (400.0, 200.0), &[], NO_SNAP),
            rect(0.2, 0.2, 0.2, 0.2),
        );
        close(
            drag_to(&d, &p, (5000.0, 5000.0), &[], NO_SNAP),
            rect(0.8, 0.8, 0.2, 0.2),
        );
        close(
            drag_to(&d, &p, (-5000.0, -5000.0), &[], NO_SNAP),
            rect(0.0, 0.0, 0.2, 0.2),
        );
    }

    #[test]
    fn a_corner_resizes_from_the_opposite_one_and_an_edge_from_one_side() {
        let start = rect(0.2, 0.2, 0.4, 0.4); // screen 300..700 × 150..350
        close(
            drag(Handle::BottomRight, start, (800.0, 400.0), NO_SNAP),
            rect(0.2, 0.2, 0.5, 0.5),
        );
        close(
            drag(Handle::TopLeft, start, (200.0, 100.0), NO_SNAP),
            rect(0.1, 0.1, 0.5, 0.5),
        );
        close(
            drag(Handle::Right, start, (800.0, 999.0), NO_SNAP),
            rect(0.2, 0.2, 0.5, 0.4),
        );
        close(
            drag(Handle::Top, start, (999.0, 100.0), NO_SNAP),
            rect(0.2, 0.1, 0.4, 0.5),
        );
    }

    #[test]
    fn a_layer_never_gets_smaller_than_the_minimum_or_flips() {
        let start = rect(0.2, 0.2, 0.4, 0.4);
        let r = drag(Handle::BottomRight, start, (0.0, 0.0), NO_SNAP);
        close(r, rect(0.2, 0.2, MIN_LAYER_SIZE, MIN_LAYER_SIZE));
        let l = drag(Handle::Left, start, (5000.0, 250.0), NO_SNAP);
        assert!((l.w - MIN_LAYER_SIZE).abs() < 1e-4);
        assert!(
            (l.x + l.w - 0.6).abs() < 1e-4,
            "the right edge stays: {l:?}"
        );
    }

    #[test]
    fn resizing_cannot_leave_the_picture() {
        let start = rect(0.5, 0.5, 0.2, 0.2);
        let r = drag(Handle::BottomRight, start, (9000.0, 9000.0), NO_SNAP);
        close(r, rect(0.5, 0.5, 0.5, 0.5));
    }

    #[test]
    fn shift_keeps_the_proportion_from_a_corner() {
        let keep = Modifiers {
            keep_aspect: true,
            no_snap: true,
        };
        let start = rect(0.2, 0.2, 0.4, 0.2); // 2:1 in the picture
        let r = drag(Handle::BottomRight, start, (900.0, 350.0), keep);
        assert!((r.w / r.h - 2.0).abs() < 1e-3, "{r:?}");
        assert!(
            (r.x - 0.2).abs() < 1e-4 && (r.y - 0.2).abs() < 1e-4,
            "the top left stays"
        );
        let up = drag(Handle::TopLeft, start, (150.0, 60.0), keep);
        assert!((up.w / up.h - 2.0).abs() < 1e-3, "{up:?}");
        assert!((up.x + up.w - 0.6).abs() < 1e-3 && (up.y + up.h - 0.4).abs() < 1e-3);
        // It still respects the picture and the minimum.
        let big = drag(Handle::BottomRight, start, (9000.0, 9000.0), keep);
        assert!(big.x + big.w <= 1.0 + 1e-4 && big.y + big.h <= 1.0 + 1e-4);
        assert!((big.w / big.h - 2.0).abs() < 1e-3);
        let tiny = drag(Handle::BottomRight, start, (0.0, 0.0), keep);
        assert!(tiny.w >= MIN_LAYER_SIZE - 1e-6 && tiny.h >= MIN_LAYER_SIZE / 2.0 - 1e-6);
    }

    #[test]
    fn a_layer_is_pulled_to_the_edges_the_centre_and_other_layers() {
        let p = picture();
        let start = rect(0.1, 0.1, 0.2, 0.2);
        let d = Drag::begin(Handle::Move, start, &p, 300.0, 150.0);
        // 3 px short of the picture's left edge: the layer lands on it.
        let pulled = drag_to(&d, &p, (203.0, 150.0), &[], Modifiers::default());
        assert!(pulled.x.abs() < 1e-5, "{pulled:?}");
        // Far from everything, nothing is pulled.
        let free = drag_to(&d, &p, (400.0 + 17.0, 200.0), &[], Modifiers::default());
        assert!((free.x - (0.1 + 117.0 / 1000.0)).abs() < 1e-4, "{free:?}");
        // To an edge of another layer.
        let other = rect(0.5, 0.5, 0.2, 0.2);
        let near = drag_to(
            &d,
            &p,
            (300.0 + 0.4 * 1000.0 + 4.0, 150.0 + 100.0),
            &[other],
            Modifiers::default(),
        );
        // Its left edge, 4 px past the other layer's, lands on it.
        assert!((near.x - 0.5).abs() < 1e-5, "{near:?}");
        // Alt (no snap) leaves it where the pointer put it.
        let alt = drag_to(&d, &p, (203.0, 150.0), &[], NO_SNAP);
        assert!((alt.x - 0.003).abs() < 1e-4, "{alt:?}");
    }

    #[test]
    fn a_resized_edge_is_pulled_too() {
        let p = picture();
        let start = rect(0.2, 0.2, 0.4, 0.4);
        let d = Drag::begin(Handle::Right, start, &p, 700.0, 250.0);
        // The right edge 4 px short of the picture's right edge.
        let r = drag_to(&d, &p, (1100.0 - 4.0, 250.0), &[], Modifiers::default());
        assert!((r.x + r.w - 1.0).abs() < 1e-5, "{r:?}");
    }
}
