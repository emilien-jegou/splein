// Single responsibility: Outer layout, placement, and visibility modifier properties for any Element.

use crate::dsl::element::Element;
use crate::foundation::*;
use crate::reactive::prop::Prop;

#[derive(Default, Clone)]
pub struct SurfaceProps {
    pub width: Option<Prop<Size>>,
    pub height: Option<Prop<Size>>,
    pub margin: Option<Margin>,
    pub x: Option<Prop<f32>>,
    pub y: Option<Prop<f32>>,
    pub z_index: Option<Prop<i32>>,
    pub is_absolute: bool,
    pub anchor: Option<Anchor>,
    pub presence: Option<Prop<LayoutPresence>>,
}

impl SurfaceProps {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn merge_margin(&mut self, m: Margin) {
        self.margin = Some(self.margin.unwrap_or(Margin::ZERO).merged(m));
    }

    pub fn apply(self, mut element: Element) -> Element {
        match &mut element {
            Element::Group(g) => {
                if let Some(w) = self.width { g.width = Some(w); }
                if let Some(h) = self.height { g.height = Some(h); }
                if let Some(m) = self.margin { g.margin = g.margin.merged(m); }
                if let Some(x) = self.x { g.x = Some(x); }
                if let Some(y) = self.y { g.y = Some(y); }
                if let Some(z) = self.z_index { g.z_index = Some(z); }
                if self.is_absolute { g.is_absolute = true; }
                if let Some(a) = self.anchor { g.anchor = Some(a); g.is_absolute = true; }
                if let Some(v) = self.presence { g.presence = Some(v); }
            }
            Element::Text(t) => {
                if let Some(w) = self.width {
                    if let Prop::Static(s) = w { t.width = Some(s); }
                }
                if let Some(h) = self.height {
                    if let Prop::Static(s) = h { t.height = Some(s); }
                }
                if let Some(x) = self.x {
                    if let Prop::Static(val) = x { t.x = Some(val); }
                }
                if let Some(y) = self.y {
                    if let Prop::Static(val) = y { t.y = Some(val); }
                }
                if let Some(m) = self.margin { t.margin = t.margin.merged(m); }
                if self.is_absolute { t.is_absolute = true; }
                if let Some(a) = self.anchor { t.anchor = Some(a); t.is_absolute = true; }
                if let Some(z) = self.z_index {
                    if let Prop::Static(val) = z { t.z_index = val; }
                }
            }
            Element::Image(i) => {
                if let Some(w) = self.width {
                    if let Prop::Static(s) = w { i.width = Some(s); }
                }
                if let Some(h) = self.height {
                    if let Prop::Static(s) = h { i.height = Some(s); }
                }
                if let Some(x) = self.x {
                    if let Prop::Static(val) = x { i.x = Some(val); }
                }
                if let Some(y) = self.y {
                    if let Prop::Static(val) = y { i.y = Some(val); }
                }
                if let Some(m) = self.margin { i.margin = i.margin.merged(m); }
                if self.is_absolute { i.is_absolute = true; }
                if let Some(a) = self.anchor { i.anchor = Some(a); i.is_absolute = true; }
                if let Some(z) = self.z_index {
                    if let Prop::Static(val) = z { i.z_index = val; }
                }
            }
            Element::Custom(c) => {
                if let Some(w) = self.width {
                    if let Prop::Static(s) = w { c.width = Some(s); }
                }
                if let Some(h) = self.height {
                    if let Prop::Static(s) = h { c.height = Some(s); }
                }
                if let Some(x) = self.x {
                    if let Prop::Static(val) = x { c.x = Some(val); }
                }
                if let Some(y) = self.y {
                    if let Prop::Static(val) = y { c.y = Some(val); }
                }
                if let Some(m) = self.margin { c.margin = c.margin.merged(m); }
                if self.is_absolute { c.is_absolute = true; }
                if let Some(a) = self.anchor { c.anchor = Some(a); c.is_absolute = true; }
                if let Some(z) = self.z_index {
                    if let Prop::Static(val) = z { c.z_index = val; }
                }
            }
        }
        element
    }
}
