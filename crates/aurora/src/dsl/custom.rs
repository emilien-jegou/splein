// Single responsibility: Declarative builder for user-defined custom primitives.

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::foundation::{Anchor, Key, Margin, Size};
use crate::tree::Primitive;
use std::rc::Rc;

/// Declarative element builder for custom primitives with layout and anchoring.
#[derive(Clone)]
pub struct CustomDef {
    pub key: Option<Key>,
    /// Shared render primitive. Cloning shares rather than duplicates: primitives
    /// are stateless (`measure`/`paint` take `&self`), so a refcount bump is exact.
    pub primitive: Rc<dyn Primitive>,
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
        primitive: Rc::new(primitive),
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
    /// Merges a margin: nonzero sides of `m` win, the rest is kept.
    ///
    /// Chained calls compose (`.margin(left(..)).margin(top(..))`), which is the
    /// only reading under which the per-side constructors make sense.
    pub fn margin(mut self, m: Margin) -> Self {
        self.margin = self.margin.merged(m);
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
