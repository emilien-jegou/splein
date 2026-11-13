// Architectural goal: Text builder definition and typography element composition.

mod setters;

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::foundation::{Anchor, Color, Key, Margin, Material, Padding, Size};
use crate::reactive::prop::{IntoProp, Prop};
use crate::text::{
    FontId, FontStyle, FontVariation, LineHeight, TextAlign, TextConfig, TextDecoration,
    TextOverflow, TextWrap,
};

/// Declarative element builder for configured typography nodes.
#[derive(Clone)]
pub struct TextElement {
    pub key: Option<Key>,
    pub content: Prop<String>,
    pub font_id: Option<FontId>,
    pub family: Option<String>,
    pub style: FontStyle,
    pub size: f32,
    pub weight: u16,
    pub variations: Vec<FontVariation>,
    pub line_height: LineHeight,
    pub letter_spacing: f32,
    pub align: TextAlign,
    pub wrap: TextWrap,
    pub decoration: TextDecoration,
    pub overflow: TextOverflow,
    pub max_lines: Option<u32>,
    pub color: Color,
    pub selectable: bool,
    pub selection_fill: Option<Material>,
    pub is_absolute: bool,
    pub anchor: Option<Anchor>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub margin: Margin,
    pub padding: Padding,
    pub width: Option<Size>,
    pub height: Option<Size>,
    pub blend: crate::foundation::BlendMode,
    pub z_index: i32,
    pub is_overlay: bool,
    pub has_layer: bool,
}

pub fn text<P: IntoProp<String>>(content: P) -> TextElement {
    TextElement {
        key: None,
        content: content.into_prop(),
        font_id: None,
        family: None,
        style: FontStyle::Normal,
        size: 14.0,
        weight: 400,
        variations: Vec::new(),
        line_height: LineHeight::Multiple(1.2),
        letter_spacing: 0.0,
        align: TextAlign::Start,
        wrap: TextWrap::Wrap,
        decoration: TextDecoration::NONE,
        overflow: TextOverflow::Clip,
        max_lines: None,
        color: Color::WHITE,
        selectable: false,
        selection_fill: None,
        is_absolute: false,
        x: None,
        y: None,
        anchor: None,
        margin: Margin::ZERO,
        padding: Padding::ZERO,
        blend: crate::foundation::BlendMode::Normal,
        width: None,
        height: None,
        z_index: 0,
        is_overlay: false,
        has_layer: false,
    }
}

impl TextElement {
    pub fn to_config(&self, resolved_content: String) -> TextConfig {
        TextConfig {
            content: resolved_content,
            font_id: self.font_id,
            family: self.family.clone(),
            style: self.style,
            size: self.size,
            weight: self.weight,
            variations: self.variations.clone(),
            line_height: self.line_height,
            letter_spacing: self.letter_spacing,
            align: self.align,
            wrap: self.wrap,
            decoration: self.decoration,
            overflow: self.overflow,
            max_lines: self.max_lines,
            color: self.color,
            selectable: self.selectable,
            selection_fill: self.selection_fill.clone(),
        }
    }
}

impl IntoElement for TextElement {
    fn into_element(self) -> Element {
        Element::Text(self)
    }
}
