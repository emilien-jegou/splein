// Single responsibility: Declarative builder for user-defined custom primitives.

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::foundation::{Anchor, Key, Margin, Padding, Size};
use crate::reactive::prop::{IntoProp, Prop};
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
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub margin: Margin,
    pub padding: Padding,
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
        x: None,
        y: None,
        margin: Margin::ZERO,
        padding: Padding::ZERO,
        anchor: None,
        is_absolute: false,
        z_index: 0,
        is_overlay: false,
        has_layer: false,
    }
}

impl CustomDef {
    /// Explicit reconciliation key for custom primitives (e.g. SVGs).
    pub fn key<K: Into<Key>>(mut self, k: K) -> Self {
        self.key = Some(k.into());
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.width = Some(Size::Fixed(w));
        self.height = Some(Size::Fixed(h));
        self
    }

    pub fn w(mut self, w: impl IntoProp<Size>) -> Self {
        if let Prop::Static(s) = w.into_prop() {
            self.width = Some(s);
        }
        self
    }

    pub fn width(self, w: impl IntoProp<Size>) -> Self {
        self.w(w)
    }

    pub fn h(mut self, h: impl IntoProp<Size>) -> Self {
        if let Prop::Static(s) = h.into_prop() {
            self.height = Some(s);
        }
        self
    }

    pub fn height(self, h: impl IntoProp<Size>) -> Self {
        self.h(h)
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

    pub fn material<M>(self, _m: M) -> Self {
        self
    }

    pub fn clip(self, _clip: impl Into<bool>) -> Self {
        self
    }

    /// Merges a margin: nonzero sides of `m` win, the rest is kept.
    pub fn margin(mut self, m: Margin) -> Self {
        self.margin = self.margin.merged(m);
        self
    }

    /// Merges a padding: nonzero sides of `p` win, the rest is kept.
    pub fn padding(mut self, p: Padding) -> Self {
        self.padding = self.padding.merged(p);
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
