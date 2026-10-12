// Single responsibility: Declarative builder for Text elements with dynamic reactive content.

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::foundation::{Anchor, Color, Key, Margin, Padding, Size};
use crate::reactive::prop::{DynamicProp, IntoProp};
use crate::text::{
    FontId, FontStyle, FontVariation, LineHeight, TextAlign, TextConfig, TextDecoration,
    TextOverflow, TextWrap,
};

/// Declarative element builder for configured typography nodes.
#[derive(Clone)]
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
    /// Variable-font axis settings applied during shaping.
    pub variations: Vec<FontVariation>,
    /// Line height policy resolved against the font size.
    pub line_height: LineHeight,
    /// Tracking offset added between characters.
    pub letter_spacing: f32,
    /// Horizontal alignment of lines within the paragraph box.
    pub align: TextAlign,
    /// Line-breaking policy: wrap (default), nowrap, or balance.
    pub wrap: TextWrap,
    /// Decoration lines drawn under or over the run.
    pub decoration: TextDecoration,
    /// Overflow behavior past the line budget.
    pub overflow: TextOverflow,
    /// Maximum visible line count, or None for unbounded.
    pub max_lines: Option<u32>,
    /// Text foreground color.
    pub color: Color,
    /// Whether this run participates in mouse text selection.
    pub selectable: bool,
    /// Highlight painted behind the selected range.
    pub selection_color: Color,
    /// Whether layout uses absolute positioning.
    pub is_absolute: bool,
    /// Optional anchoring target for positioning.
    pub anchor: Option<Anchor>,
    /// Surrounding margin offsets.
    pub margin: Margin,
    /// Internal padding inset. Grows the text box; the run paints at the
    /// padding origin (top-left inset).
    pub padding: Padding,
    /// Layout width sizing intent (Fit wraps to content, Fill/Fixed constrain wrap).
    pub width: Option<Size>,
    /// Layout height sizing intent.
    pub height: Option<Size>,
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
        selection_color: crate::text::DEFAULT_SELECTION_COLOR,
        is_absolute: false,
        anchor: None,
        margin: Margin::ZERO,
        padding: Padding::ZERO,
        width: None,
        height: None,
        z_index: 0,
        is_overlay: false,
        has_layer: false,
    }
}

impl TextDef {
    /// Whether the content reads context (a `DynamicProp::Context`).
    /// Reconcile uses it to place missing-provider guards.
    pub fn has_context_reads(&self) -> bool {
        matches!(self.content, crate::reactive::prop::DynamicProp::Context(_))
    }

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
    /// Sets one variable-font axis by 4-character tag (`"wght"`, `"opsz"`, ...).
    ///
    /// Unknown tags are ignored so a typo never breaks the builder chain.
    pub fn font_variation(mut self, tag: &str, value: f32) -> Self {
        if let Some(setting) = FontVariation::named(tag, value) {
            self.variations.retain(|v| v.tag != setting.tag);
            self.variations.push(setting);
        }
        self
    }
    /// Sets several variable-font axes at once (see [`TextDef::font_variation`]).
    pub fn font_variations(mut self, settings: &[(&str, f32)]) -> Self {
        for (tag, value) in settings {
            self = self.font_variation(tag, *value);
        }
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
    /// Sets horizontal paragraph alignment.
    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }
    /// Sets the line-breaking policy (`TextWrap`, `"wrap"` / `"nowrap"` / `"balance"`).
    pub fn text_wrap(mut self, wrap: impl Into<TextWrap>) -> Self {
        self.wrap = wrap.into();
        self
    }
    /// Sets the decoration line set.
    pub fn decoration(mut self, decoration: TextDecoration) -> Self {
        self.decoration = decoration;
        self
    }
    /// Enables underlining.
    pub fn underline(mut self) -> Self {
        self.decoration = self.decoration | TextDecoration::UNDERLINE;
        self
    }
    /// Enables strikethrough.
    pub fn strikethrough(mut self) -> Self {
        self.decoration = self.decoration | TextDecoration::LINE_THROUGH;
        self
    }
    /// Enables overline.
    pub fn overline(mut self) -> Self {
        self.decoration = self.decoration | TextDecoration::OVERLINE;
        self
    }
    /// Clamps the visible line count, ellipsizing the last line when exceeded.
    pub fn max_lines(mut self, lines: u32) -> Self {
        self.max_lines = Some(lines);
        self.overflow = TextOverflow::Ellipsis;
        self
    }
    /// Sets the overflow behavior past the line budget.
    pub fn overflow(mut self, overflow: TextOverflow) -> Self {
        self.overflow = overflow;
        self
    }
    /// Sets text foreground color (`Color`, `"#121212"`, `"red"`, `0x121212`).
    pub fn color(mut self, c: impl Into<Color>) -> Self {
        self.color = c.into();
        self
    }
    /// Opts this run into mouse text selection (readonly highlight + drag).
    pub fn selectable(mut self, on: bool) -> Self {
        self.selectable = on;
        self
    }
    /// Sets the highlight painted behind the selected range.
    pub fn selection_color(mut self, c: Color) -> Self {
        self.selection_color = c;
        self
    }
    /// Sets surrounding margin offset.
    /// Merges a margin: nonzero sides of `m` win, the rest is kept.
    ///
    /// Chained calls compose (`.margin(left(..)).margin(top(..))`), which is the
    /// only reading under which the per-side constructors make sense.
    pub fn margin(mut self, m: impl Into<Margin>) -> Self {
        self.margin = self.margin.merged(m.into());
        self
    }
    /// Merges a padding: nonzero sides of `p` win, the rest is kept.
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = self.padding.merged(p.into());
        self
    }
    /// Sets the layout width sizing intent.
    pub fn width(mut self, w: impl Into<Size>) -> Self {
        self.width = Some(w.into());
        self
    }
    /// Sets the layout height sizing intent.
    pub fn height(mut self, h: impl Into<Size>) -> Self {
        self.height = Some(h.into());
        self
    }

    /// Enables absolute coordinate layout.
    pub fn absolute(mut self) -> Self {
        self.is_absolute = true;
        self
    }
    /// Sets anchor alignment and enables absolute layout.
    pub fn anchor(mut self, a: impl Into<Anchor>) -> Self {
        self.anchor = Some(a.into());
        self.is_absolute = true;
        self
    }
    /// Sets display list stacking z-index.
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }
    /// Toggles membership in the overlay plane.
    pub fn overlay(mut self, on: bool) -> Self {
        self.is_overlay = on;
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
            selection_color: self.selection_color,
        }
    }
}

impl IntoElement for TextDef {
    fn into_element(self) -> Element {
        Element::Text(self)
    }
}
