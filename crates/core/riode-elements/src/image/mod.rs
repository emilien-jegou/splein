// Responsibility: First-class declarative builder for raster images with object-fit scaling.

pub mod loader;
pub mod raster;
pub mod task;

pub use loader::load_asset;
pub use raster::ImageProcessor;
pub use task::{ImageTask, IMAGE_KIND};

use riode_core::dsl::component::IntoElement;
use riode_core::dsl::element::Element;
use riode_core::dsl::leaf::{LeafBuilder, LeafProps};
use riode_core::foundation::ImageFit;
use riode_core::scene::image::ImageSource;
use crate::config::ImageConfig;

#[derive(Clone)]
pub struct ImageElement {
    pub props: LeafProps,
    pub source: Option<ImageSource>,
    pub fit: ImageFit,
}

pub fn image(source: ImageSource) -> ImageElement {
    ImageElement {
        props: LeafProps { aspect_ratio: source.aspect_ratio(), ..LeafProps::default() },
        source: Some(source),
        fit: ImageFit::Cover,
    }
}

pub fn build() -> ImageElement {
    ImageElement {
        props: LeafProps::default(),
        source: None,
        fit: ImageFit::Cover,
    }
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

    pub fn src(mut self, source: ImageSource) -> Self {
        self.props.aspect_ratio = source.aspect_ratio();
        self.source = Some(source);
        self
    }

    pub fn to_config(&self) -> ImageConfig {
        ImageConfig {
            image: self.source.clone().unwrap_or_else(ImageSource::empty),
            fit: self.fit,
        }
    }

    pub fn into_leaf(mut self) -> riode_core::dsl::leaf_def::LeafDef {
        if let Some(source) = &self.source {
            if self.props.width.is_none() && self.props.height.is_none() {
                let (w, h) = (source.width as f32, source.height as f32);
                self.props.width = Some(riode_core::foundation::Size::fixed(w));
                self.props.height = Some(riode_core::foundation::Size::fixed(h));
            }
        }
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
    fn from(def: ImageElement) -> Self {
        Element::Leaf(def.into_leaf())
    }
}
