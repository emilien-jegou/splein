// Single responsibility: Configuration and layout parameters for styled typography.

use crate::foundation::Color;
use crate::text::fonts::FontId;

/// Configuration specification for rendering shaped text runs.
#[derive(Clone, Debug, PartialEq)]
pub struct TextConfig {
    /// String content to render.
    pub content: String,
    /// Explicit font identifier or fallback.
    pub font_id: Option<FontId>,
    /// Font size in logical pixels.
    pub size: f32,
    /// Numerical font weight.
    pub weight: u16,
    /// Line height in logical pixels.
    pub line_height: f32,
    /// Tracking offset added between characters.
    pub letter_spacing: f32,
    /// Text foreground color.
    pub color: Color,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            content: String::new(),
            font_id: None,
            size: 14.0,
            weight: 400,
            line_height: 20.0,
            letter_spacing: 0.0,
            color: Color::WHITE,
        }
    }
}
