// Single responsibility: First-class declarative builder for raster images with object-fit scaling.

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::foundation::{Anchor, AspectRatio, ImageFit, Key, Margin, Padding, Size};
use crate::reactive::prop::{IntoProp, Prop};
use crate::scene::image::ImageSource;
use crate::tree::kind::ImageConfig;

#[derive(Clone)]
pub struct ImageElement {
    pub source: ImageSource,
    pub fit: ImageFit,
    pub width: Option<Size>,
    pub height: Option<Size>,
    pub aspect_ratio: Option<f32>,
    pub margin: Margin,
    pub padding: Padding,
    pub is_absolute: bool,
    pub anchor: Option<Anchor>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z_index: i32,
    pub key: Option<Key>,
}

pub fn image(source: ImageSource) -> ImageElement {
    let (w, h) = (source.width as f32, source.height as f32);
    let ratio = if w > 0.0 && h > 0.0 { Some(w / h) } else { None };
    ImageElement {
        source,
        fit: ImageFit::Cover,
        width: None,
        height: None,
        aspect_ratio: ratio,
        margin: Margin::ZERO,
        padding: Padding::ZERO,
        is_absolute: false,
        anchor: None,
        x: None,
        y: None,
        z_index: 0,
        key: None,
    }
}

impl ImageElement {
    pub fn key<K: Into<Key>>(mut self, k: K) -> Self {
        self.key = Some(k.into());
        self
    }

    pub fn fit(mut self, fit: impl Into<ImageFit>) -> Self {
        self.fit = fit.into();
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

    pub fn ratio(mut self, ratio: impl Into<AspectRatio>) -> Self {
        self.aspect_ratio = Some(ratio.into().ratio());
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

    pub fn material<M>(self, _m: M) -> Self {
        self
    }

    pub fn clip(self, _clip: impl Into<bool>) -> Self {
        self
    }

    pub fn margin(mut self, m: Margin) -> Self {
        self.margin = self.margin.merged(m);
        self
    }

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

    pub fn to_config(&self) -> ImageConfig {
        ImageConfig {
            image: self.source.clone(),
            fit: self.fit,
        }
    }
}

impl IntoElement for ImageElement {
    fn into_element(self) -> Element {
        Element::Image(self)
    }
}
