//! What the interface hands to the renderer: a list of coloured quads and the
//! glyph bitmaps to put into the atlas. Plain data, so the code that builds a
//! frame of interface and the code that draws it on the GPU never see each
//! other.

/// Atlas texture edge length in pixels.
pub const ATLAS_SIZE: u32 = 2048;

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

/// Pixels waiting to be copied into the atlas texture.
pub struct GlyphUpload {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

/// One frame of interface, ready to draw.
pub struct DrawList<'a> {
    /// Triangles, in physical pixels.
    pub vertices: &'a [Vertex],
    /// Glyph bitmaps that are new since the last frame.
    pub uploads: Vec<GlyphUpload>,
    /// Size of the picture being drawn on, in physical pixels.
    pub target_size: (u32, u32),
}
