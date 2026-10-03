// Single responsibility: Configuration and layout parameters for styled typography.

use crate::foundation::Color;
use crate::text::align::TextAlign;
use crate::text::decoration::TextDecoration;
use crate::text::fonts::{FontId, FontStyle};
use crate::text::overflow::TextOverflow;

/// Line height policy: fixed pixels or a multiple of the font size.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LineHeight {
    /// Fixed line height in logical pixels.
    Absolute(f32),
    /// Line height as a multiplier of the font size.
    Multiple(f32),
}

impl LineHeight {
    /// Resolves the policy to an absolute pixel line height for a font size.
    pub fn resolve(self, size: f32) -> f32 {
        let effective = if size.is_finite() && size > 0.0 {
            size
        } else {
            14.0
        };
        match self {
            LineHeight::Absolute(value) if value.is_finite() && value > 0.0 => value,
            LineHeight::Multiple(mult) if mult.is_finite() && mult > 0.0 => effective * mult,
            _ => effective * 1.2,
        }
    }
}

impl Default for LineHeight {
    fn default() -> Self {
        Self::Multiple(1.2)
    }
}

/// Configuration specification for rendering shaped text runs.
#[derive(Clone, Debug, PartialEq)]
pub struct TextConfig {
    /// String content to render.
    pub content: String,
    /// Explicit font identifier override, resolved by database index.
    pub font_id: Option<FontId>,
    /// Preferred font family name resolved through the font database.
    pub family: Option<String>,
    /// Font style selecting normal, italic, or oblique faces.
    pub style: FontStyle,
    /// Font size in logical pixels.
    pub size: f32,
    /// Numerical font weight.
    pub weight: u16,
    /// Line height policy resolved against the font size.
    pub line_height: LineHeight,
    /// Tracking offset added between characters.
    pub letter_spacing: f32,
    /// Horizontal alignment of lines within the paragraph box.
    pub align: TextAlign,
    /// Decoration lines drawn under or over the run.
    pub decoration: TextDecoration,
    /// Overflow behavior past the line budget.
    pub overflow: TextOverflow,
    /// Maximum visible line count, or None for unbounded.
    pub max_lines: Option<u32>,
    /// Text foreground color.
    pub color: Color,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            content: String::new(),
            font_id: None,
            family: None,
            style: FontStyle::Normal,
            size: 14.0,
            weight: 400,
            line_height: LineHeight::Multiple(1.2),
            letter_spacing: 0.0,
            align: TextAlign::Start,
            decoration: TextDecoration::NONE,
            overflow: TextOverflow::Clip,
            max_lines: None,
            color: Color::WHITE,
        }
    }
}
