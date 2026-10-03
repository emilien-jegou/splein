// Single responsibility: Trait contracts and parameter structs for multi-line text shaping.

use crate::foundation::Constraints;
use crate::text::fonts::{FontId, FontStyle};
use crate::text::layout::TextLayout;

/// Unified parameter specification for text layout and shaping passes.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TextShapeParams<'a> {
    /// Source text slice to shape.
    pub text: &'a str,
    /// Target font identifier or None for default.
    pub font: Option<FontId>,
    /// Preferred font family name or None for the generic default.
    pub family: Option<&'a str>,
    /// Font style selecting normal, italic, or oblique faces.
    pub style: FontStyle,
    /// Font size in logical pixels.
    pub size: f32,
    /// Resolved absolute line height in logical pixels.
    pub line_height: f32,
    /// Additional tracking interval between characters.
    pub letter_spacing: f32,
    /// Numeric font weight.
    pub weight: u16,
    /// Layout constraints for line wrapping and bounding.
    pub constraints: Constraints,
}

/// Trait contract for shaping strings into multi-line glyph layouts.
pub trait TextShaper {
    /// Shapes a text string into multi-line glyph runs.
    fn shape(&self, params: TextShapeParams) -> TextLayout;
}
