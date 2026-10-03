// Single responsibility: Shaped glyph metrics, font cache key, and line runs.

use crate::foundation::Point;

/// Backend-agnostic cache identifier for an individual shaped glyph run.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GlyphKey(pub u64);

/// Metrics and placement coordinates for an individual shaped glyph.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedGlyph {
    pub glyph_id: u32,
    pub point: Point,
    pub advance: f32,
    pub cluster: usize,
    pub cache_key: GlyphKey,
    /// Index of the resolved font blob used to render this glyph.
    pub font_blob: usize,
}

/// A shaped horizontal run of glyphs with line geometry metrics.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedLine {
    pub glyphs: Vec<ShapedGlyph>,
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub font_size: f32,
}

impl ShapedLine {
    /// Constructs a shaped line run with explicit typographic metrics.
    pub fn new(
        glyphs: Vec<ShapedGlyph>,
        width: f32,
        height: f32,
        baseline: f32,
        font_size: f32,
    ) -> Self {
        Self {
            glyphs,
            width,
            height,
            baseline,
            font_size,
        }
    }
}
