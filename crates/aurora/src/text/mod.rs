// Single responsibility: Typography domain exports, font database, and shaping engine.

pub mod align;
pub mod cache;
pub mod config;
pub mod context;
pub mod decoration;
pub mod engine;
pub mod fonts;
pub mod glyph;
pub mod glyph_cache;
pub mod layout;
pub mod leaf;
pub(crate) mod min_content;
pub mod overflow;
pub mod runs;
pub mod selection;
pub mod shape;
pub mod shaper;

pub use align::TextAlign;
pub use cache::ShapedTextCache;
pub use config::{LineHeight, TextConfig};
pub use context::{GlyphBitmap, TextContext, SMALL_TEXT_HINT_CUTOFF_PX};
pub use decoration::TextDecoration;
pub use engine::ParleyEngine;
pub use fonts::{FontData, FontId, FontStyle, FontVariation};
pub use glyph_cache::{GlyphBitmapCache, GlyphCacheBudget, GlyphCacheStats};
pub use glyph::{GlyphKey, ShapedGlyph, ShapedLine};
pub use layout::TextLayout;
pub use overflow::TextOverflow;
pub use selection::{
    caret_at_point, caret_line_col, caret_rect, expand_selection, grapheme_boundaries,
    is_word_char, line_byte_ranges, move_grapheme, offset_at_visual_line, selection_rects,
    snap_to_boundary, word_bounds, word_edge, SelectionGranularity, TextSelection,
    CARET_WIDTH, DEFAULT_SELECTION_COLOR,
};
pub use shaper::{TextShapeParams, TextShaper};
