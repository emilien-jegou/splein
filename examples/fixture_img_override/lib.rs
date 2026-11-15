// Responsibility: Acceptance fixture 2 — a crate's own `img` replacing the built-in.
//
// The local `img` module shadows the `riode::prelude::*` glob, so `<img>` in
// `view!` lowers to *this* crate's element, not the built-in. No fork, no
// registration: plain name resolution at the call site.

use riode::dsl::{Element, IntoElement, LeafBuilder, LeafDef, LeafProps};
use riode::foundation::{Constraints, IntrinsicSize, ResolvedRect};
use riode::prelude::Primitive;
use riode::scene::PaintContext;
use riode::scene::image::ImageSource;
use riode::tree::kind::{LeafChange, MeasureCtx};
use riode::tree::{DirtyFlags, NodeState, NodeStyle};

/// The override's leaf: same pixels, `contain` fit — the built-in covers
/// (crops); this one letterboxes. Different behavior, same tag.
pub struct FitImage {
    image: ImageSource,
}

impl Primitive for FitImage {
    fn measure(&self, _style: &NodeStyle, _state: &mut NodeState, ctx: &MeasureCtx) -> IntrinsicSize {
        let (iw, ih) = (self.image.width as f32, self.image.height as f32);
        if iw <= 0.0 || ih <= 0.0 {
            return IntrinsicSize { width: 0.0, height: 0.0 };
        }
        let ratio = iw / ih;
        if ctx.constraints.max_width.is_finite() && ctx.constraints.max_width > 0.0 {
            let w = ctx.constraints.max_width;
            return IntrinsicSize { width: w, height: w / ratio };
        }
        IntrinsicSize { width: iw, height: ih }
    }

    fn min_content(&self, _style: &NodeStyle, _axis: riode::foundation::Direction, _text: &riode::text::TextContext) -> f32 {
        self.image.width as f32
    }

    fn paint(&self, _style: &NodeStyle, _state: &mut NodeState, ctx: &mut PaintContext, bounds: ResolvedRect) {
        let (iw, ih) = (self.image.width as f32, self.image.height as f32);
        if iw <= 0.0 || ih <= 0.0 || bounds.width <= 0.0 || bounds.height <= 0.0 {
            return;
        }
        let scale = (bounds.width / iw).min(bounds.height / ih);
        let (w, h) = (iw * scale, ih * scale);
        let x = bounds.x + (bounds.width - w) * 0.5;
        let y = bounds.y + (bounds.height - h) * 0.5;
        ctx.draw_image(ResolvedRect::new(x, y, w, h), self.image.clone());
    }

    fn reconcile(&self, next: &dyn Primitive) -> Option<LeafChange> {
        next.as_any().downcast_ref::<FitImage>()?;
        Some(LeafChange {
            dirty: DirtyFlags::MEASURE | DirtyFlags::LAYOUT,
            ..LeafChange::default()
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone)]
pub struct ImgElement {
    props: LeafProps,
    image: Option<ImageSource>,
}

impl LeafBuilder for ImgElement {
    fn leaf(&mut self) -> &mut LeafProps {
        &mut self.props
    }
}

impl ImgElement {
    pub fn src(mut self, image: ImageSource) -> Self {
        let (w, h) = (image.width as f32, image.height as f32);
        self.props.aspect_ratio = (w > 0.0 && h > 0.0).then(|| w / h);
        self.image = Some(image);
        self
    }

    pub fn into_leaf(self) -> LeafDef {
        let mut def = LeafDef::new(FitImage {
            image: self.image.unwrap_or_else(ImageSource::empty),
        });
        def.props = self.props;
        def
    }
}

impl IntoElement for ImgElement {
    fn into_element(self) -> Element {
        Element::Leaf(self.into_leaf())
    }
}

/// Shadows the built-in `img` glob: this crate's `img::build()` wins.
pub mod img {
    pub fn build() -> super::ImgElement {
        super::ImgElement { props: super::LeafProps::default(), image: None }
    }
}

pub mod view;

pub use view::Page;
