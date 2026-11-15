// Single responsibility: Fluid property setters, typography modifiers, and DSL shorthands.

use super::TextElement;
use riode_core::foundation::{Color, Material};
use riode_core::text::{
    FontId, FontStyle, FontVariation, LineHeight, TextAlign, TextDecoration, TextOverflow, TextWrap,
};

impl TextElement {
    pub fn font(mut self, f: FontId) -> Self {
        self.font_id = Some(f);
        self
    }
    /// Replaces the text content, including with a reactive source.
    pub fn content(mut self, content: impl riode_core::reactive::prop::IntoProp<String>) -> Self {
        self.content = content.into_prop();
        self
    }
    /// Sets the child content of this element — the `view!` lowering of
    /// `<text>Hello</text>`. Alias of [`content`](Self::content) with the
    /// generic contract name every lowercase tag's builder shares.
    pub fn child(mut self, child: impl riode_core::reactive::prop::IntoProp<String>) -> Self {
        self.content = child.into_prop();
        self
    }
    pub fn font_family(mut self, name: impl Into<String>) -> Self {
        self.family = Some(name.into());
        self
    }
    pub fn blend(mut self, b: impl Into<riode_core::foundation::BlendMode>) -> Self {
        self.blend = b.into();
        self
    }
    pub fn font_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
    pub fn italic(mut self, on: impl Into<bool>) -> Self {
        self.style = if on.into() { FontStyle::Italic } else { FontStyle::Normal };
        self
    }
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    pub fn s(self, size: impl riode_core::dsl::prop::Px) -> Self {
        self.size(size.px())
    }
    pub fn weight(mut self, w: u16) -> Self {
        self.weight = w;
        self
    }
    pub fn font_variation(mut self, tag: &str, value: f32) -> Self {
        if let Some(setting) = FontVariation::named(tag, value) {
            self.variations.retain(|v| v.tag != setting.tag);
            self.variations.push(setting);
        }
        self
    }
    pub fn font_variations(mut self, settings: &[(&str, f32)]) -> Self {
        for (tag, value) in settings {
            self = self.font_variation(tag, *value);
        }
        self
    }
    pub fn line_height(mut self, lh: impl Into<LineHeight>) -> Self {
        self.line_height = lh.into();
        self
    }
    pub fn line_height_multiple(mut self, mult: f32) -> Self {
        self.line_height = LineHeight::Multiple(mult);
        self
    }
    pub fn letter_spacing(mut self, ls: f32) -> Self {
        self.letter_spacing = ls;
        self
    }
    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }
    pub fn align(self, a: impl Into<TextAlign>) -> Self {
        self.text_align(a.into())
    }
    pub fn text_wrap(mut self, wrap: impl Into<TextWrap>) -> Self {
        self.wrap = wrap.into();
        self
    }
    pub fn decoration(mut self, decoration: TextDecoration) -> Self {
        self.decoration = decoration;
        self
    }
    pub fn underline(mut self, on: impl Into<bool>) -> Self {
        if on.into() {
            self.decoration = self.decoration | TextDecoration::UNDERLINE;
        }
        self
    }
    pub fn strikethrough(mut self, on: impl Into<bool>) -> Self {
        if on.into() {
            self.decoration = self.decoration | TextDecoration::LINE_THROUGH;
        }
        self
    }
    pub fn overline(mut self, on: impl Into<bool>) -> Self {
        if on.into() {
            self.decoration = self.decoration | TextDecoration::OVERLINE;
        }
        self
    }
    pub fn max_lines(mut self, lines: impl riode_core::dsl::prop::Count) -> Self {
        self.max_lines = Some(lines.count());
        self.overflow = TextOverflow::Ellipsis;
        self
    }
    pub fn overflow(mut self, overflow: TextOverflow) -> Self {
        self.overflow = overflow;
        self
    }
    pub fn color(mut self, c: impl Into<Color>) -> Self {
        self.color = c.into();
        self
    }
    pub fn selectable(mut self, on: bool) -> Self {
        self.selectable = on;
        self
    }
    pub fn selection_fill(mut self, fill: impl Into<Material>) -> Self {
        self.selection_fill = Some(fill.into());
        self
    }
}
