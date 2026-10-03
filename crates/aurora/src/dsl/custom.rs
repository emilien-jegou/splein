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
    pub fn anchor(mut self, a: Anchor) -> Self {
        self.anchor = Some(a);
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

impl IntoElement for CustomDef {
    fn into_element(self) -> Element {
        Element::Custom(self)
    }
}
