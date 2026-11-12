//! Immediate-mode drawing layer: shapes and text are queued as vertices in
//! logical pixels and drawn by [`gpu::UiGpu`] on top of the video.

pub mod controls;
pub mod gpu;
pub mod input;
pub mod text;

use crate::theme::Color;
use text::{Face, TextSystem, Weight};

/// One corner of a UI quad. Physical pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 2],
    /// Rounded rects: position relative to the rect centre (px).
    /// Glyphs: atlas texture coordinates (0..1).
    pub uv: [f32; 2],
    pub color: [f32; 4],
    /// Rounded rects: `(half_w, half_h, radius, blur)`.
    /// Glyphs: `z < 0`.
    pub shape: [f32; 4],
}

/// A rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Typographic style for [`Ui::text`].
#[derive(Debug, Clone, Copy)]
pub struct TextStyle {
    /// Logical pixels.
    pub size: f32,
    pub weight: Weight,
    /// Fixed-width digits, for values that change (fps, ms, %).
    pub tabular: bool,
}

impl TextStyle {
    pub const fn new(size: f32, weight: Weight) -> Self {
        Self {
            size,
            weight,
            tabular: false,
        }
    }

    pub const fn tabular(self) -> Self {
        Self {
            tabular: true,
            ..self
        }
    }
}

/// Collects one frame of drawing commands.
pub struct Ui {
    pub text: TextSystem,
    vertices: Vec<Vertex>,
    scale: f32,
    viewport: (u32, u32),
    opacity: f32,
    /// Logical-pixel translation applied to everything drawn (for slides).
    offset: (f32, f32),
}

impl Ui {
    pub fn new() -> Self {
        Self {
            text: TextSystem::new(),
            vertices: Vec::new(),
            scale: 1.0,
            viewport: (1, 1),
            opacity: 1.0,
            offset: (0.0, 0.0),
        }
    }

    /// Starts a frame. `viewport` is in physical pixels; `scale` converts
    /// logical to physical pixels.
    pub fn begin(&mut self, viewport: (u32, u32), scale: f32) {
        self.vertices.clear();
        self.viewport = viewport;
        self.scale = scale.max(0.5);
        self.opacity = 1.0;
        self.offset = (0.0, 0.0);
    }

    /// Translates everything drawn from now on, in logical pixels.
    pub fn set_offset(&mut self, dx: f32, dy: f32) {
        self.offset = (dx, dy);
    }

    /// Multiplies the alpha of everything drawn from now on (for fades).
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
    }

    fn tint(&self, color: Color) -> [f32; 4] {
        color.with_alpha(color.a * self.opacity).to_array()
    }

    /// Viewport size in logical pixels.
    pub fn logical_size(&self) -> (f32, f32) {
        (
            self.viewport.0 as f32 / self.scale,
            self.viewport.1 as f32 / self.scale,
        )
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn viewport(&self) -> (u32, u32) {
        self.viewport
    }

    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// A filled rounded rectangle.
    pub fn rect(&mut self, r: Rect, radius: f32, color: Color) {
        self.sdf_quad(r, radius, 0.0, 1.0, color);
    }

    /// A rounded rectangle with a 1-physical-pixel-aligned border drawn
    /// *inside* its bounds.
    pub fn panel(&mut self, r: Rect, radius: f32, fill: Color, border: Color, border_width: f32) {
        self.rect(r, radius, border);
        let inset = border_width;
        let inner = Rect::new(
            r.x + inset,
            r.y + inset,
            (r.w - 2.0 * inset).max(0.0),
            (r.h - 2.0 * inset).max(0.0),
        );
        self.rect(inner, (radius - inset).max(0.0), fill);
    }

    /// A soft shadow around the outside of a rounded rectangle.
    pub fn shadow(&mut self, r: Rect, radius: f32, blur: f32, color: Color) {
        self.sdf_quad(r, radius, blur, blur * 2.0, color);
    }

    fn sdf_quad(&mut self, r: Rect, radius: f32, blur: f32, margin: f32, color: Color) {
        let s = self.scale;
        let (w, h) = (r.w * s, r.h * s);
        let (x, y) = ((r.x + self.offset.0) * s, (r.y + self.offset.1) * s);
        let pad = margin * s.max(1.0) + 1.0;
        let (half_w, half_h) = (w / 2.0, h / 2.0);
        let shape = [
            half_w,
            half_h,
            (radius * s).min(half_w.min(half_h)),
            blur * s,
        ];
        let rgba = self.tint(color);
        let (cx, cy) = (x + half_w, y + half_h);
        let corners = [
            [x - pad, y - pad],
            [x + w + pad, y - pad],
            [x + w + pad, y + h + pad],
            [x - pad, y + h + pad],
        ];
        let quad: Vec<Vertex> = corners
            .iter()
            .map(|&[px, py]| Vertex {
                pos: [px, py],
                uv: [px - cx, py - cy],
                color: rgba,
                shape,
            })
            .collect();
        self.push_quad(&quad);
    }

    fn push_quad(&mut self, q: &[Vertex]) {
        self.vertices
            .extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
    }

    fn face(style: TextStyle) -> Face {
        Face::Text(style.weight)
    }

    /// Size of `s` in logical pixels.
    pub fn measure(&mut self, s: &str, style: TextStyle) -> (f32, f32) {
        let shaped = self
            .text
            .shape(s, style.size * self.scale, Self::face(style), style.tabular);
        (shaped.width / self.scale, shaped.height / self.scale)
    }

    /// Draws `s` with its top-left at `(x, y)`; returns its logical width.
    pub fn text(&mut self, s: &str, x: f32, y: f32, style: TextStyle, color: Color) -> f32 {
        self.draw_run(s, x, y, style.size, Self::face(style), style.tabular, color)
    }

    /// Draws a Lucide icon centred on `(cx, cy)`.
    pub fn icon(&mut self, glyph: char, cx: f32, cy: f32, size: f32, color: Color) {
        let s = glyph.to_string();
        let shaped = self.text.shape(&s, size * self.scale, Face::Icons, false);
        let (w, h) = (shaped.width / self.scale, shaped.height / self.scale);
        self.draw_run(
            &s,
            cx - w / 2.0,
            cy - h / 2.0,
            size,
            Face::Icons,
            false,
            color,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_run(
        &mut self,
        s: &str,
        x: f32,
        y: f32,
        size: f32,
        face: Face,
        tabular: bool,
        color: Color,
    ) -> f32 {
        let scale = self.scale;
        let shaped = self.text.shape(s, size * scale, face, tabular);
        // Snap the origin to whole pixels so glyph edges stay crisp.
        let (ox, oy) = (
            ((x + self.offset.0) * scale).round(),
            ((y + self.offset.1) * scale).round(),
        );
        let atlas = text::ATLAS_SIZE as f32;
        let rgba = self.tint(color);

        for glyph in &shaped.glyphs {
            let Some(slot) = self.text.slot(glyph.key) else {
                continue;
            };
            let gx = ox + (glyph.x + slot.left) as f32;
            let gy = oy + (glyph.y - slot.top) as f32;
            let (gw, gh) = (slot.w as f32, slot.h as f32);
            let (u0, v0) = (slot.x as f32 / atlas, slot.y as f32 / atlas);
            let (u1, v1) = (
                (slot.x + slot.w) as f32 / atlas,
                (slot.y + slot.h) as f32 / atlas,
            );
            let vertex = |px: f32, py: f32, u: f32, v: f32| Vertex {
                pos: [px, py],
                uv: [u, v],
                color: rgba,
                shape: [0.0, 0.0, -1.0, 0.0],
            };
            self.push_quad(&[
                vertex(gx, gy, u0, v0),
                vertex(gx + gw, gy, u1, v0),
                vertex(gx + gw, gy + gh, u1, v1),
                vertex(gx, gy + gh, u0, v1),
            ]);
        }
        shaped.width / scale
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn ui() -> Ui {
        let mut ui = Ui::new();
        ui.begin((1280, 720), 1.0);
        ui
    }

    #[test]
    fn a_rect_is_two_triangles() {
        let mut ui = ui();
        ui.rect(Rect::new(10.0, 10.0, 100.0, 40.0), 8.0, Theme::dark().text);
        assert_eq!(ui.vertices().len(), 6);
    }

    #[test]
    fn rect_quad_covers_its_bounds_with_an_antialiasing_margin() {
        let mut ui = ui();
        ui.rect(Rect::new(10.0, 20.0, 100.0, 40.0), 8.0, Theme::dark().text);
        let xs: Vec<f32> = ui.vertices().iter().map(|v| v.pos[0]).collect();
        let (min, max) = (
            xs.iter().cloned().fold(f32::MAX, f32::min),
            xs.iter().cloned().fold(f32::MIN, f32::max),
        );
        assert!(min < 10.0 && max > 110.0);
        // The local coordinate at the quad centre is (0, 0).
        let v = ui.vertices()[0];
        assert!((v.uv[0] - (v.pos[0] - 60.0)).abs() < 1e-3);
    }

    #[test]
    fn scale_converts_logical_to_physical_pixels() {
        let mut ui = Ui::new();
        ui.begin((2560, 1440), 2.0);
        ui.rect(Rect::new(10.0, 10.0, 100.0, 40.0), 8.0, Theme::dark().text);
        assert_eq!(ui.vertices()[0].shape[0], 100.0); // half of 200 px
        assert_eq!(ui.logical_size(), (1280.0, 720.0));
    }

    #[test]
    fn radius_is_clamped_to_half_the_shortest_side() {
        let mut ui = ui();
        ui.rect(Rect::new(0.0, 0.0, 100.0, 20.0), 999.0, Theme::dark().text);
        assert_eq!(ui.vertices()[0].shape[2], 10.0);
    }

    #[test]
    fn text_emits_one_quad_per_visible_glyph_and_returns_its_width() {
        let mut ui = ui();
        let style = TextStyle::new(13.0, Weight::Regular);
        let width = ui.text("Live", 0.0, 0.0, style, Theme::dark().text);
        assert_eq!(ui.vertices().len(), 4 * 6);
        assert!(width > 10.0);
        assert!(ui.vertices().iter().all(|v| v.shape[2] < 0.0));
    }

    #[test]
    fn spaces_draw_nothing() {
        let mut ui = ui();
        let style = TextStyle::new(13.0, Weight::Regular);
        ui.text("a b", 0.0, 0.0, style, Theme::dark().text);
        assert_eq!(ui.vertices().len(), 2 * 6);
    }

    #[test]
    fn panel_draws_border_then_fill() {
        let mut ui = ui();
        let t = Theme::dark();
        ui.panel(
            Rect::new(0.0, 0.0, 50.0, 50.0),
            10.0,
            t.surface,
            t.border,
            1.0,
        );
        assert_eq!(ui.vertices().len(), 12);
        assert_eq!(ui.vertices()[0].color, t.border.to_array());
        assert_eq!(ui.vertices()[6].color, t.surface.to_array());
    }

    #[test]
    fn offset_translates_shapes_and_text_and_resets_each_frame() {
        let mut ui = ui();
        ui.rect(Rect::new(10.0, 10.0, 20.0, 20.0), 2.0, Theme::dark().text);
        let plain = ui.vertices()[0].pos;
        ui.begin((1280, 720), 1.0);
        ui.set_offset(5.0, 7.0);
        ui.rect(Rect::new(10.0, 10.0, 20.0, 20.0), 2.0, Theme::dark().text);
        let moved = ui.vertices()[0].pos;
        assert_eq!((moved[0] - plain[0], moved[1] - plain[1]), (5.0, 7.0));
        ui.begin((1280, 720), 1.0);
        ui.rect(Rect::new(10.0, 10.0, 20.0, 20.0), 2.0, Theme::dark().text);
        assert_eq!(ui.vertices()[0].pos, plain);
    }

    #[test]
    fn rect_contains_is_half_open() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(r.contains(0.0, 0.0) && r.contains(9.9, 9.9));
        assert!(!r.contains(10.0, 5.0));
    }
}
