// Single responsibility: Typography domain exports, font database, and shaping engine.

pub mod align;
pub(crate) mod buffer_pool;
pub mod cache;
pub mod config;
pub mod context;
pub mod cosmic;
pub mod decoration;
pub mod fonts;
pub mod glyph;
pub mod layout;
pub(crate) mod min_content;
pub mod overflow;
pub mod runs;
pub mod shape;
pub mod shaper;

pub use align::TextAlign;
pub use cache::ShapedTextCache;
pub use config::{LineHeight, TextConfig};
pub use context::{GlyphBitmap, TextContext};
pub use cosmic::CosmicTextEngine;
pub use decoration::TextDecoration;
pub use fonts::{FontData, FontId, FontStyle};
pub use glyph::{GlyphKey, ShapedGlyph, ShapedLine};
pub use layout::TextLayout;
pub use overflow::TextOverflow;
pub use shaper::{TextShapeParams, TextShaper};
