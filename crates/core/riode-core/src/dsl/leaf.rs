// Single responsibility: Shared layout/positioning property block and fluent builder for leaf elements.

use crate::foundation::{Anchor, AspectRatio, Key, Margin, Padding, Size};
use crate::reactive::prop::{IntoProp, Prop};

/// The outer layout properties every leaf element shares: sizing, spacing,
/// anchoring and z-ordering. Built-ins and third-party primitives carry the
/// same block, so the reconciler treats all leaves uniformly.
#[derive(Clone)]
pub struct LeafProps {
    pub key: Option<Key>,
    pub width: Option<Size>,
    pub height: Option<Size>,
    pub aspect_ratio: Option<f32>,
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

impl Default for LeafProps {
    fn default() -> Self {
        Self {
            key: None,
            width: None,
            height: None,
            aspect_ratio: None,
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
}

impl LeafProps {
    /// Effective z-index: explicit value wins, else absolute layers sit above flow.
    pub fn effective_z(&self) -> i32 {
        if self.z_index != 0 { self.z_index } else if self.is_absolute { 1 } else { 0 }
    }
}

/// Fluent leaf property setters, shared verbatim by `text`, `img`/`svg` and
/// custom primitives. Each leaf builder implements `leaf()` once; every setter
/// is defined here, so no per-type hand-written copy exists.
///
/// On `TextElement` the inherent `size(s)` (font size) intentionally shadows
/// the trait's two-argument `size(w, h)`; text is not size-specified that way.
pub trait LeafBuilder: Sized {
    fn leaf(&mut self) -> &mut LeafProps;

    fn key<K: Into<Key>>(mut self, k: K) -> Self {
        self.leaf().key = Some(k.into());
        self
    }

    fn size(mut self, w: f32, h: f32) -> Self {
        let p = self.leaf();
        p.width = Some(Size::Fixed(w));
        p.height = Some(Size::Fixed(h));
        self
    }

    fn w(mut self, w: impl IntoProp<Size>) -> Self {
        if let Prop::Static(s) = w.into_prop() {
            self.leaf().width = Some(s);
        }
        self
    }

    fn width(self, w: impl IntoProp<Size>) -> Self {
        self.w(w)
    }

    fn h(mut self, h: impl IntoProp<Size>) -> Self {
        if let Prop::Static(s) = h.into_prop() {
            self.leaf().height = Some(s);
        }
        self
    }

    fn height(self, h: impl IntoProp<Size>) -> Self {
        self.h(h)
    }

    fn ratio(mut self, ratio: impl Into<AspectRatio>) -> Self {
        self.leaf().aspect_ratio = Some(ratio.into().ratio());
        self
    }

    fn absolute(mut self, is_abs: impl Into<bool>) -> Self {
        self.leaf().is_absolute = is_abs.into();
        self
    }

    fn x(mut self, x: impl IntoProp<f32>) -> Self {
        let p = self.leaf();
        if let Prop::Static(v) = x.into_prop() {
            p.x = Some(v);
        }
        p.is_absolute = true;
        self
    }

    fn y(mut self, y: impl IntoProp<f32>) -> Self {
        let p = self.leaf();
        if let Prop::Static(v) = y.into_prop() {
            p.y = Some(v);
        }
        p.is_absolute = true;
        self
    }

    fn margin(mut self, m: impl Into<Margin>) -> Self {
        let p = self.leaf();
        p.margin = p.margin.merged(m.into());
        self
    }

    fn padding(mut self, pd: impl Into<Padding>) -> Self {
        let p = self.leaf();
        p.padding = p.padding.merged(pd.into());
        self
    }

    fn anchor(mut self, a: impl Into<Anchor>) -> Self {
        let p = self.leaf();
        p.anchor = Some(a.into());
        p.is_absolute = true;
        self
    }

    fn z_index(mut self, z: i32) -> Self {
        self.leaf().z_index = z;
        self
    }

    fn overlay(mut self, on: bool) -> Self {
        self.leaf().is_overlay = on;
        self
    }
}
