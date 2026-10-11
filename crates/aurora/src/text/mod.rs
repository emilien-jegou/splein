//! Typography: shaping, fonts, glyphs, selection — everything text-shaped.
//!
//! Text is the only leaf the layout engine cannot measure by itself: a paragraph's size
//! depends on fonts, shaping, wrapping, and alignment. This module owns that entire
//! pipeline — font database → shaper (Parley) → line runs → glyph raster cache — plus
//! the selection queries the input layer drives. Layout treats text as opaque sizing
//! behavior behind [`leaf`] methods; it never inspects glyphs itself.
//!
//! # The pipeline
//!
//! [`TextContext`] (font DB + shaping dispatcher + glyph rasterizer) shapes a
//! [`TextConfig`] into a [`TextLayout`] (lines + total size) via
//! [`TextContext::shape_config`]. Layouts are memoized in [`ShapedTextCache`] (keyed by
//! content + width, quantized so near-identical widths share entries); bitmaps are
//! memoized in [`GlyphBitmapCache`] under an adaptive budget. Backends consume layouts
//! by emitting [`SceneCommand::DrawText`](crate::scene::SceneCommand); they never shape.
//!
//! # Showcase
//!
//! Shaping directly (what the layout engine does internally on every text node):
//!
//! ```ignore
//! use aurora::prelude::*;
//! use aurora::text::{TextConfig, TextContext};
//! use aurora::foundation::Constraints;
//!
//! let ctx = TextContext::new();
//! ctx.load_font(include_bytes!("inter-var.ttf"));
//! let layout = ctx.shape_config(
//!     &TextConfig { content: "Hello, world".to_string(), size: 16.0, ..Default::default() },
//!     Constraints::loose(200.0, f32::INFINITY),
//! );
//! assert!(!layout.lines.is_empty());
//! ```
//!
//! Selection is mouse-only and readonly: [`selection::caret_at_point`] maps a point to a
//! byte offset, double-click expands by word, triple-click by paragraph, and
//! [`selection::selection_rects`] yields per-line highlight boxes. Keyboard and clipboard
//! are deliberately out of scope (see [`crate::input::selection`]).
//!
//! # Module map
//!
//! | Module | Holds |
//! |--------|-------|
//! | [`context`] | [`TextContext`]: the shaper/rasterizer facade everything enters through |
//! | [`engine`] | [`ParleyEngine`]: Parley wrapper, font loading, family resolution |
//! | [`config`] | [`TextConfig`], [`LineHeight`]: styling input |
//! | [`layout`] | [`TextLayout`]: shaped lines + total size (the unit everything passes around) |
//! | [`runs`] / [`shape`] / [`shaper`] | Line extraction, shaping entry, [`TextShapeParams`]/[`TextShaper`] trait |
//! | [`glyph`] / [`glyph_cache`] | [`ShapedGlyph`], bitmap memoization |
//! | [`cache`] | [`ShapedTextCache`]: shaped-layout memoization |
//! | [`fonts`] | [`FontId`], [`FontStyle`], font data |
//! | [`leaf`] | The layout-leaf bridge: wrap widths, min-content, paint emission |
//! | [`selection`] | Selection model + hit queries ([`TextSelection`], caret/selection rects) |
//! | [`align`] / [`decoration`] / [`overflow`] | [`TextAlign`], [`TextDecoration`], [`TextOverflow`] + helpers |
//!
//! # Pitfalls
//!
//! - Shaping is expensive; the caches are the performance story. A `TextConfig` built
//!   fresh per frame with identical content still hits the cache — but a *different*
//!   content string re-shapes, so keep content in signals, not in rebuilt strings.
//! - Glyph bitmaps are keyed by allocation identity behind `Arc`: same glyphs, new
//!   allocation → cache miss. Retained layouts (not reshaped clones) keep hit rates up.
//! - `TextLayout` is immutable and shareable. If you want to mutate text, mutate the
//!   `TextConfig` and reshape — never the layout.

pub mod align;
pub mod analytic;
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
pub use context::{quantize_raster_scale, GlyphBitmap, TextContext, SMALL_TEXT_HINT_CUTOFF_PX};
pub use decoration::TextDecoration;
pub use engine::ParleyEngine;
pub use fonts::{FontData, FontId, FontStyle, FontVariation};
pub use glyph::{GlyphKey, ShapedGlyph, ShapedLine};
pub use glyph_cache::{GlyphBitmapCache, GlyphCacheBudget, GlyphCacheStats};
pub use layout::TextLayout;
pub use overflow::TextOverflow;
pub use selection::{
    caret_at_point, caret_line_col, caret_rect, expand_selection, grapheme_boundaries,
    is_word_char, line_byte_ranges, move_grapheme, offset_at_visual_line, selection_rects,
    snap_to_boundary, word_bounds, word_edge, SelectionGranularity, TextSelection, CARET_WIDTH,
    DEFAULT_SELECTION_COLOR,
};
pub use shaper::{TextShapeParams, TextShaper};
