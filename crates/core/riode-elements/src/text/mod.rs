// Architectural goal: Text builder definition and typography element composition.

mod setters;

use riode_core::dsl::component::IntoElement;
use riode_core::dsl::element::Element;
use riode_core::dsl::leaf::{LeafBuilder, LeafProps};
use riode_core::foundation::{Color, Material};
use riode_core::reactive::prop::{IntoProp, Prop};
use riode_core::text::leaf::TextLeaf;
use riode_core::text::{
    FontId, FontStyle, FontVariation, LineHeight, TextAlign, TextConfig, TextDecoration,
    TextOverflow, TextWrap,
};
use riode_core::tree::binding::DynamicBindings;
use std::rc::Rc;

/// Declarative element builder for configured typography nodes.
#[derive(Clone)]
pub struct TextElement {
    pub props: LeafProps,
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
    pub blend: riode_core::foundation::BlendMode,
}

pub fn text<P: IntoProp<String>>(content: P) -> TextElement {
    TextElement {
        props: LeafProps::default(),
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
        blend: riode_core::foundation::BlendMode::Normal,
    }
}

/// Tag-namespace constructor for the `view!` macro: `<text>` lowers to
/// `text::build()` and fills itself through setters (`.child(...)`,
/// `.content(...)`, `size`, `color`, …). A module lives in the type
/// namespace, so the path survives any same-name value in scope.
pub fn build() -> TextElement {
    TextElement {
        props: LeafProps::default(),
        content: Prop::Static(String::new()),
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
        blend: riode_core::foundation::BlendMode::Normal,
    }
}

impl LeafBuilder for TextElement {
    fn leaf(&mut self) -> &mut LeafProps {
        &mut self.props
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

impl TextElement {
    /// Lowers the builder into the erased leaf shape every element shares.
    ///
    /// Static content resolves here; dynamic content resolves once at
    /// reconcile (observer-attributed) and then per frame through the
    /// scheduler's binding pass.
    pub fn into_leaf(self) -> riode_core::dsl::leaf_def::LeafDef {
        let dynamic = self.content.is_dynamic();
        let resolved = match &self.content {
            Prop::Static(s) => s.clone(),
            _ => String::new(),
        };
        let config = self.to_config(resolved);
        riode_core::dsl::leaf_def::LeafDef {
            props: self.props,
            primitive: Rc::new(TextLeaf::new(config)),
            bindings: DynamicBindings { text: self.content.dynamic_closure(), ..Default::default() },
            dynamic,
        }
    }
}

impl IntoElement for TextElement {
    fn into_element(self) -> Element {
        Element::Leaf(self.into_leaf())
    }
}

impl From<TextElement> for riode_core::dsl::Element {
    fn from(def: TextElement) -> Self { Element::Leaf(def.into_leaf()) }
}
