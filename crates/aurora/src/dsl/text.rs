// Single responsibility: Declarative builder for Text elements with dynamic reactive content.

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::foundation::{Anchor, Color, Key, Margin};
use crate::reactive::prop::{DynamicProp, IntoProp};
use crate::text::{FontId, FontStyle, LineHeight, TextConfig};

/// Declarative element builder for configured typography nodes.
pub struct TextDef {
    /// Optional diffing reconciliation key.
    pub key: Option<Key>,
    /// Dynamic or static string text content.
    pub content: DynamicProp<String>,
    /// Optional explicit font face override.
    pub font_id: Option<FontId>,
    /// Optional preferred font family name.
    pub family: Option<String>,
    /// Font style (normal, italic, oblique).
    pub style: FontStyle,
    /// Font size in logical pixels.
    pub size: f32,
    /// Font weight parameter.
    pub weight: u16,
    /// Line height policy resolved against the font size.
    pub line_height: LineHeight,
    /// Tracking offset added between characters.
    pub letter_spacing: f32,
    /// Text foreground color.
    pub color: Color,
    /// Whether layout uses absolute positioning.
    pub is_absolute: bool,
    /// Optional anchoring target for positioning.
    pub anchor: Option<Anchor>,
    /// Surrounding margin offsets.
    pub margin: Margin,
    /// Rendering stacking order index.
    pub z_index: i32,
    /// Whether element hovers in the overlay plane.
    pub is_overlay: bool,
    /// Whether element subtree is cached in a layer.
    pub has_layer: bool,
}

/// Constructs a text element builder with reactive or static content.
pub fn text<P: IntoProp<String>>(content: P) -> TextDef {
    TextDef {
        key: None,
        content: content.into_prop(),
        font_id: None,
        family: None,
        style: FontStyle::Normal,
        size: 14.0,
        weight: 400,
        line_height: LineHeight::Multiple(1.2),
        letter_spacing: 0.0,
        color: Color::WHITE,
        is_absolute: false,
        anchor: None,
        margin: Margin::ZERO,
        z_index: 0,
        is_overlay: false,
        has_layer: false,
    }
}

impl TextDef {
    /// Sets an explicit reconciliation diffing key.
    pub fn key<K: Into<Key>>(mut self, k: K) -> Self {
        self.key = Some(k.into());
        self
    }
    /// Sets explicit font face override.
    pub fn font(mut self, f: FontId) -> Self {
        self.font_id = Some(f);
        self
    }
    /// Sets the preferred font family name, resolved by the database.
    pub fn font_family(mut self, name: impl Into<String>) -> Self {
        self.family = Some(name.into());
        self
    }
    /// Sets the font style (normal, italic, oblique).
    pub fn font_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
    /// Selects the italic face.
    pub fn italic(mut self) -> Self {
        self.style = FontStyle::Italic;
        self
    }
    /// Sets font size in logical pixels.
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    /// Sets numerical font weight.
    pub fn weight(mut self, w: u16) -> Self {
        self.weight = w;
        self
    }
    /// Sets absolute line height in logical pixels.
    pub fn line_height(mut self, lh: f32) -> Self {
        self.line_height = LineHeight::Absolute(lh);
        self
    }
    /// Sets line height as a multiple of the font size.
    pub fn line_height_multiple(mut self, mult: f32) -> Self {
        self.line_height = LineHeight::Multiple(mult);
        self
    }
    /// Sets letter tracking spacing.
    pub fn letter_spacing(mut self, ls: f32) -> Self {
        self.letter_spacing = ls;
        self
    }
    /// Sets text foreground color.
    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }
    /// Sets surrounding margin offset.
    pub fn margin(mut self, m: Margin) -> Self {
        self.margin = m;
        self
    }
    /// Sets symmetric horizontal and vertical margins.
    pub fn margin_xy(self, x: f32, y: f32) -> Self {
        self.margin_x(x).margin_y(y)
    }
    /// Sets horizontal margin offset.
    pub fn margin_x(mut self, x: f32) -> Self {
        self.margin.left = x;
        self.margin.right = x;
        self
    }
    /// Sets vertical margin offset.
    pub fn margin_y(mut self, y: f32) -> Self {
        self.margin.top = y;
        self.margin.bottom = y;
        self
    }
    /// Sets left margin offset.
    pub fn margin_left(mut self, l: f32) -> Self {
        self.margin.left = l;
        self
    }
    /// Sets top margin offset.
    pub fn margin_top(mut self, t: f32) -> Self {
        self.margin.top = t;
        self
    }
    /// Sets right margin offset.
    pub fn margin_right(mut self, r: f32) -> Self {
        self.margin.right = r;
        self
    }
    /// Sets bottom margin offset.
    pub fn margin_bottom(mut self, b: f32) -> Self {
        self.margin.bottom = b;
        self
    }
    /// Enables absolute coordinate layout.
    pub fn absolute(mut self) -> Self {
        self.is_absolute = true;
        self
    }
    /// Sets anchor alignment and enables absolute layout.
    pub fn anchor(mut self, a: Anchor) -> Self {
        self.anchor = Some(a);
        self.is_absolute = true;
        self
    }
    /// Sets display list stacking z-index.
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }
    /// Flags element as an overlay drawn above base tree.
    pub fn overlay(mut self) -> Self {
        self.is_overlay = true;
        self.z_index = 100;
        self
    }

    /// Converts builder into pure immutable configuration specification.
    pub fn to_config(&self, resolved_content: String) -> TextConfig {
        TextConfig {
            content: resolved_content,
            font_id: self.font_id,
            family: self.family.clone(),
            style: self.style,
            size: self.size,
            weight: self.weight,
            line_height: self.line_height,
            letter_spacing: self.letter_spacing,
            color: self.color,
        }
    }
}

impl IntoElement for TextDef {
    fn into_element(self) -> Element {
        Element::Text(self)
    }
}
