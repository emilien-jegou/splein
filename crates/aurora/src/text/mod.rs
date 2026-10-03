// Single responsibility: Typography domain exports, font database, and shaping engine.

pub mod cache;
pub mod config;
pub mod context;
pub mod cosmic;
pub mod fonts;
pub mod glyph;
pub mod layout;
pub mod shaper;

pub use cache::ShapedTextCache;
pub use config::{LineHeight, TextConfig};
pub use context::{GlyphBitmap, TextContext};
pub use cosmic::CosmicTextEngine;
pub use fonts::{FontData, FontId, FontStyle};
pub use glyph::{GlyphKey, ShapedGlyph, ShapedLine};
pub use layout::TextLayout;
pub use shaper::{TextShapeParams, TextShaper};
