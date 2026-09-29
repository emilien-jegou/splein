// Single responsibility: Declarative builder for user-defined custom primitives.

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::foundation::{Anchor, Key, Margin, Size};
use crate::tree::Primitive;

/// Declarative element builder for custom primitives with layout and anchoring.
pub struct CustomDef {
    pub key: Option<Key>,
    pub primitive: Box<dyn Primitive>,
    pub width: Option<Size>,
    pub height: Option<Size>,
    pub margin: Margin,
    pub anchor: Option<Anchor>,
    pub is_absolute: bool,
    pub z_index: i32,
    pub is_overlay: bool,
    pub has_layer: bool,
}

pub fn custom<P: Primitive + 'static>(primitive: P) -> CustomDef {
    CustomDef {
        key: None,
        primitive: Box::new(primitive),
        width: None,
        height: None,
        margin: Margin::ZERO,
        anchor: None,
        is_absolute: false,
        z_index: 0,
        is_overlay: false,
        has_layer: false,
    }
}

impl CustomDef {
    /// FIX: explicit reconciliation key for custom primitives (e.g. SVGs).
    pub fn key<K: Into<Key>>(mut self, k: K) -> Self {
        self.key = Some(k.into());
        self
    }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(Size::Fixed(w));
        self.height = Some(Size::Fixed(h));
        self
    }
    pub fn margin(mut self, m: Margin) -> Self {
        self.margin = m;
        self
    }
    pub fn margin_x(mut self, x: f32) -> Self {
        self.margin.left = x;
        self.margin.right = x;
        self
    }
    pub fn margin_y(mut self, y: f32) -> Self {
        self.margin.top = y;
        self.margin.bottom = y;
        self
    }
    pub fn margin_xy(self, x: f32, y: f32) -> Self {
        self.margin_x(x).margin_y(y)
    }
    pub fn margin_left(mut self, l: f32) -> Self {
        self.margin.left = l;
        self
    }
    pub fn margin_top(mut self, t: f32) -> Self {
        self.margin.top = t;
        self
    }
    pub fn margin_right(mut self, r: f32) -> Self {
        self.margin.right = r;
        self
    }
    pub fn margin_bottom(mut self, b: f32) -> Self {
        self.margin.bottom = b;
        self
    }
    pub fn anchor(mut self, a: Anchor) -> Self {
        self.anchor = Some(a);
        self.is_absolute = true;
        self
    }
    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }
    pub fn overlay(mut self) -> Self {
        self.is_overlay = true;
        self.z_index = 100;
        self
    }
}

impl IntoElement for CustomDef {
    fn into_element(self) -> Element {
        Element::Custom(self)
    }
}
