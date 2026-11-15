// Single responsibility: First-class declarative builder for raster images with object-fit scaling.

use riode_core::dsl::component::IntoElement;
use riode_core::dsl::element::Element;
use riode_core::dsl::leaf::{LeafBuilder, LeafProps};
use riode_core::foundation::ImageFit;
use riode_core::scene::image::ImageSource;
use crate::config::ImageConfig;

#[derive(Clone)]
pub struct ImageElement {
    pub props: LeafProps,
    /// `None` until a `src` is set — `<img />` without one renders nothing.
    pub source: Option<ImageSource>,
    pub fit: ImageFit,
}

pub fn image(source: ImageSource) -> ImageElement {
    ImageElement {
        props: LeafProps { aspect_ratio: natural_ratio(&source), ..LeafProps::default() },
        source: Some(source),
        fit: ImageFit::Cover,
    }
}

/// Tag-namespace constructor for the `view!` macro: `<img>` lowers to
/// `img::build()` and fills itself through setters (`.src(...)`, `w`, `h`, …).
pub fn build() -> ImageElement {
    ImageElement {
        props: LeafProps::default(),
        source: None,
        fit: ImageFit::Cover,
    }
}

fn natural_ratio(source: &ImageSource) -> Option<f32> {
    let (w, h) = (source.width as f32, source.height as f32);
    (w > 0.0 && h > 0.0).then(|| w / h)
}

impl LeafBuilder for ImageElement {
    fn leaf(&mut self) -> &mut LeafProps {
        &mut self.props
    }
}

impl ImageElement {
    pub fn fit(mut self, fit: impl Into<ImageFit>) -> Self {
        self.fit = fit.into();
        self
    }

    /// Sets the pixels this element draws. The source of `<img src=…>` and the
    /// only way a zero-arg `img::build()` gets content.
    pub fn src(mut self, source: ImageSource) -> Self {
        self.props.aspect_ratio = natural_ratio(&source);
        self.source = Some(source);
        self
    }

    pub fn to_config(&self) -> ImageConfig {
        ImageConfig {
            image: self.source.clone().unwrap_or_else(ImageSource::empty),
            fit: self.fit,
        }
    }

    /// Lowers the builder into the erased leaf shape every element shares.
    pub fn into_leaf(self) -> riode_core::dsl::leaf_def::LeafDef {
        let mut def = riode_core::dsl::leaf_def::LeafDef::new(self.to_config());
        def.props = self.props;
        def
    }
}

impl IntoElement for ImageElement {
    fn into_element(self) -> Element {
        Element::Leaf(self.into_leaf())
    }
}

impl From<ImageElement> for riode_core::dsl::Element {
    fn from(def: ImageElement) -> Self { Element::Leaf(def.into_leaf()) }
}
