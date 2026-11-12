//! Text shaping (cosmic-text) and the glyph atlas bookkeeping.
//!
//! Glyph bitmaps are packed into a single-channel atlas; this module decides
//! *where* each glyph goes and queues the pixel data for the GPU to upload.

use std::collections::HashMap;

use cosmic_text::{
    fontdb, Attrs, Buffer, CacheKey, Family, FeatureTag, FontFeatures, FontSystem, Metrics,
    Shaping, SwashCache, SwashContent, Weight as FontWeight,
};

const INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../assets/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");
const LUCIDE: &[u8] = include_bytes!("../../assets/fonts/lucide.ttf");

const TEXT_FAMILY: &str = "Inter";
const ICON_FAMILY: &str = "lucide";

/// Atlas texture edge length in pixels.
pub const ATLAS_SIZE: u32 = 2048;
/// Line height as a multiple of the font size.
const LINE_HEIGHT: f32 = 1.3;
/// Entries kept in the shaping cache before it is flushed.
const SHAPE_CACHE_LIMIT: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
}

/// Which font a run of text is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Text(Weight),
    Icons,
}

/// Where a glyph's bitmap lives in the atlas, plus its draw offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphSlot {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    /// Offset of the bitmap's left edge from the glyph origin.
    pub left: i32,
    /// Offset of the bitmap's top edge above the baseline.
    pub top: i32,
}

/// Pixels waiting to be copied into the atlas texture.
pub struct Upload {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct ShapedGlyph {
    pub key: CacheKey,
    pub x: i32,
    pub y: i32,
}

/// A shaped line of text, in physical pixels.
#[derive(Clone)]
pub struct Shaped {
    pub glyphs: Vec<ShapedGlyph>,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ShapeKey {
    text: String,
    size_px: u32,
    face: Face,
    tabular: bool,
}

/// Simple shelf packer for the atlas.
struct Packer {
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

impl Packer {
    fn new() -> Self {
        Self {
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
        }
    }

    /// Reserves a `w`×`h` rectangle (plus 1 px padding) or returns `None`
    /// when the atlas is full.
    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let (pw, ph) = (w + 1, h + 1);
        if pw > ATLAS_SIZE || ph > ATLAS_SIZE {
            return None;
        }
        if self.cursor_x + pw > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + ph > ATLAS_SIZE {
            return None;
        }
        let at = (self.cursor_x, self.cursor_y);
        self.cursor_x += pw;
        self.row_height = self.row_height.max(ph);
        Some(at)
    }
}

pub struct TextSystem {
    fonts: FontSystem,
    swash: SwashCache,
    shaped: HashMap<ShapeKey, Shaped>,
    slots: HashMap<CacheKey, Option<GlyphSlot>>,
    packer: Packer,
    uploads: Vec<Upload>,
}

impl TextSystem {
    /// Loads the bundled fonts only: no system font scan, so startup is fast
    /// and the look is identical on every machine.
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        for data in [INTER_REGULAR, INTER_MEDIUM, INTER_SEMIBOLD, LUCIDE] {
            db.load_font_data(data.to_vec());
        }
        Self {
            fonts: FontSystem::new_with_locale_and_db("en-US".into(), db),
            swash: SwashCache::new(),
            shaped: HashMap::new(),
            slots: HashMap::new(),
            packer: Packer::new(),
            uploads: Vec::new(),
        }
    }

    /// Shapes `text` at `size_px` physical pixels.
    pub fn shape(&mut self, text: &str, size_px: f32, face: Face, tabular: bool) -> Shaped {
        let key = ShapeKey {
            text: text.to_owned(),
            size_px: size_px.to_bits(),
            face,
            tabular,
        };
        if let Some(hit) = self.shaped.get(&key) {
            return hit.clone();
        }
        if self.shaped.len() >= SHAPE_CACHE_LIMIT {
            self.shaped.clear();
        }

        let (family, weight) = match face {
            Face::Text(Weight::Regular) => (TEXT_FAMILY, FontWeight::NORMAL),
            Face::Text(Weight::Medium) => (TEXT_FAMILY, FontWeight::MEDIUM),
            Face::Text(Weight::SemiBold) => (TEXT_FAMILY, FontWeight::SEMIBOLD),
            Face::Icons => (ICON_FAMILY, FontWeight::NORMAL),
        };
        let mut features = FontFeatures::new();
        if tabular {
            features.enable(FeatureTag::new(b"tnum"));
        }
        let attrs = Attrs::new()
            .family(Family::Name(family))
            .weight(weight)
            .font_features(features);

        let mut buffer = Buffer::new(
            &mut self.fonts,
            Metrics::new(size_px, size_px * LINE_HEIGHT),
        );
        buffer.set_size(None, None);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fonts, false);

        let mut shaped = Shaped {
            glyphs: Vec::new(),
            width: 0.0,
            height: size_px * LINE_HEIGHT,
        };
        for run in buffer.layout_runs() {
            shaped.width = shaped.width.max(run.line_w);
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, run.line_y), 1.0);
                shaped.glyphs.push(ShapedGlyph {
                    key: physical.cache_key,
                    x: physical.x,
                    y: physical.y,
                });
            }
        }
        self.shaped.insert(key, shaped.clone());
        shaped
    }

    /// Returns the atlas slot for a glyph, rasterising and queueing its
    /// bitmap for upload on first use. `None` for empty or non-mask glyphs.
    pub fn slot(&mut self, key: CacheKey) -> Option<GlyphSlot> {
        if let Some(cached) = self.slots.get(&key) {
            return *cached;
        }
        let slot = self.rasterise(key);
        self.slots.insert(key, slot);
        slot
    }

    fn rasterise(&mut self, key: CacheKey) -> Option<GlyphSlot> {
        let image = self.swash.get_image(&mut self.fonts, key).as_ref()?;
        if image.content != SwashContent::Mask {
            return None;
        }
        let (w, h) = (image.placement.width, image.placement.height);
        if w == 0 || h == 0 {
            return None;
        }
        let (left, top) = (image.placement.left, image.placement.top);
        let data = image.data.clone();
        let (x, y) = self.packer.alloc(w, h)?;
        self.uploads.push(Upload { x, y, w, h, data });
        Some(GlyphSlot {
            x,
            y,
            w,
            h,
            left,
            top,
        })
    }

    /// Takes the bitmaps queued since the last call.
    pub fn take_uploads(&mut self) -> Vec<Upload> {
        std::mem::take(&mut self.uploads)
    }
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fonts_provide_the_expected_families() {
        let mut text = TextSystem::new();
        let inter = text.shape("Live", 13.0, Face::Text(Weight::Regular), false);
        assert_eq!(inter.glyphs.len(), 4);
        assert!(inter.width > 10.0);
        // Lucide's volume icon lives in the private-use area.
        let icon = text.shape("\u{e1ab}", 18.0, Face::Icons, false);
        assert_eq!(icon.glyphs.len(), 1);
        assert!(
            text.slot(icon.glyphs[0].key).is_some(),
            "icon has no bitmap"
        );
    }

    #[test]
    fn spanish_accents_and_enye_render() {
        let mut text = TextSystem::new();
        let shaped = text.shape(
            "Sincronía · Español · Calidad",
            13.0,
            Face::Text(Weight::Regular),
            false,
        );
        let with_bitmap = shaped
            .glyphs
            .iter()
            .filter(|g| text.slot(g.key).is_some())
            .count();
        let visible = "Sincronía · Español · Calidad"
            .chars()
            .filter(|c| !c.is_whitespace())
            .count();
        assert_eq!(with_bitmap, visible);
        assert!(shaped.width > 100.0);
    }

    #[test]
    fn tabular_figures_have_equal_widths() {
        let mut text = TextSystem::new();
        let face = Face::Text(Weight::Regular);
        let one = text.shape("1111", 13.0, face, true).width;
        let eight = text.shape("8888", 13.0, face, true).width;
        assert!((one - eight).abs() < 0.01, "{one} vs {eight}");
        // Proportional figures differ: that is why tabular matters.
        let p_one = text.shape("1111", 13.0, face, false).width;
        let p_eight = text.shape("8888", 13.0, face, false).width;
        assert!((p_one - p_eight).abs() > 0.5, "{p_one} vs {p_eight}");
    }

    #[test]
    fn heavier_weights_are_wider() {
        let mut text = TextSystem::new();
        let regular = text
            .shape("Settings", 13.0, Face::Text(Weight::Regular), false)
            .width;
        let semibold = text
            .shape("Settings", 13.0, Face::Text(Weight::SemiBold), false)
            .width;
        assert!(semibold > regular);
    }

    #[test]
    fn same_glyph_reuses_its_atlas_slot() {
        let mut text = TextSystem::new();
        // The same glyph at the same pen position has the same cache key.
        // (Within "aa" the second glyph sits at a different sub-pixel offset,
        // so cosmic-text rightly rasterises it separately.)
        let face = Face::Text(Weight::Regular);
        let first = text.shape("a", 13.0, face, false);
        let again = text.shape("a", 13.0, face, true);
        let slot_a = text.slot(first.glyphs[0].key);
        let slot_b = text.slot(again.glyphs[0].key);
        assert!(slot_a.is_some());
        assert_eq!(slot_a, slot_b);
        assert_eq!(text.take_uploads().len(), 1);
    }

    #[test]
    fn packer_wraps_rows_and_reports_full() {
        let mut packer = Packer::new();
        assert_eq!(packer.alloc(100, 20), Some((0, 0)));
        assert_eq!(packer.alloc(100, 10), Some((101, 0)));
        // A glyph that does not fit on the row starts a new one.
        assert_eq!(packer.alloc(ATLAS_SIZE - 100, 10), Some((0, 21)));
        assert_eq!(packer.alloc(ATLAS_SIZE + 1, 1), None);
        assert_eq!(packer.alloc(10, ATLAS_SIZE), None);
    }
}
