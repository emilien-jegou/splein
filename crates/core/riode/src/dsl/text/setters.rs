// Single responsibility: Fluid property setters, typography modifiers, and DSL shorthands.

use super::TextElement;
use crate::foundation::{Anchor, Color, Key, Margin, Material, Padding, Size};
use crate::reactive::{IntoProp, Prop};
use crate::text::{
    FontId, FontStyle, FontVariation, LineHeight, TextAlign, TextDecoration, TextOverflow, TextWrap,
};

impl From<f32> for LineHeight {
    fn from(v: f32) -> Self {
        LineHeight::Absolute(v)
    }
}
impl From<i32> for LineHeight {
    fn from(v: i32) -> Self {
        LineHeight::Absolute(v as f32)
    }
}
impl From<u32> for LineHeight {
    fn from(v: u32) -> Self {
        LineHeight::Absolute(v as f32)
    }
}

impl TextElement {
    pub fn key<K: Into<Key>>(mut self, k: K) -> Self {
        self.key = Some(k.into());
        self
    }
    pub fn font(mut self, f: FontId) -> Self {
        self.font_id = Some(f);
        self
    }
    pub fn font_family(mut self, name: impl Into<String>) -> Self {
        self.family = Some(name.into());
        self
    }
    pub fn blend(mut self, b: impl Into<crate::foundation::BlendMode>) -> Self {
        self.blend = b.into();
        self
    }
    pub fn font_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
    pub fn italic(mut self) -> Self {
        self.style = FontStyle::Italic;
        self
    }
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    pub fn s(self, size: impl crate::dsl::prop::Px) -> Self {
        self.size(size.px())
    }
    pub fn weight(mut self, w: u16) -> Self {
        self.weight = w;
        self
    }
    pub fn w(self, w: impl Into<Size>) -> Self {
        self.width(w)
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
    pub fn underline(mut self) -> Self {
        self.decoration = self.decoration | TextDecoration::UNDERLINE;
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.decoration = self.decoration | TextDecoration::LINE_THROUGH;
        self
    }
    pub fn overline(mut self) -> Self {
        self.decoration = self.decoration | TextDecoration::OVERLINE;
        self
    }
    pub fn max_lines(mut self, lines: impl crate::dsl::prop::Count) -> Self {
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
    pub fn margin(mut self, m: impl Into<Margin>) -> Self {
        self.margin = self.margin.merged(m.into());
        self
    }
    pub fn padding(mut self, p: impl Into<Padding>) -> Self {
        self.padding = self.padding.merged(p.into());
        self
    }
    pub fn width(mut self, w: impl Into<Size>) -> Self {
        self.width = Some(w.into());
        self
    }
    pub fn height(mut self, h: impl Into<Size>) -> Self {
        self.height = Some(h.into());
        self
    }
    pub fn absolute(mut self, is_abs: impl Into<bool>) -> Self {
        self.is_absolute = is_abs.into();
        self
    }
    pub fn x(mut self, x: impl IntoProp<f32>) -> Self {
        if let Prop::Static(v) = x.into_prop() {
            self.x = Some(v);
        }
        self.is_absolute = true;
        self
    }

    pub fn y(mut self, y: impl IntoProp<f32>) -> Self {
        if let Prop::Static(v) = y.into_prop() {
            self.y = Some(v);
        }
        self.is_absolute = true;
        self
    }
    pub fn anchor(mut self, a: impl Into<Anchor>) -> Self {
        self.anchor = Some(a.into());
        self.is_absolute = true;
        self
    }
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }
    pub fn overlay(mut self, on: bool) -> Self {
        self.is_overlay = on;
        self
    }
}
