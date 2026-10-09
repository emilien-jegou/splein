// Single responsibility: First-class declarative builder for raster images.

use crate::dsl::custom::{custom, CustomDef};
use aurora_foundation::{Constraints, IntrinsicSize, ResolvedRect};
use crate::scene::context::PaintContext;
use crate::scene::image::ImageSource;
use crate::tree::Primitive;

struct ImagePrimitive {
    image: ImageSource,
}

impl Primitive for ImagePrimitive {
    fn measure(&self, _constraints: Constraints) -> IntrinsicSize {
        IntrinsicSize {
            width: self.image.width as f32,
            height: self.image.height as f32,
        }
    }

    fn paint(&self, ctx: &mut PaintContext, bounds: ResolvedRect) {
        ctx.draw_image(bounds, self.image.clone());
    }
}

/// Constructs a first-class declarative image element.
///
/// Sized by content like [`svg`](crate::dsl::svg): the pixel dimensions are the `Fit`
/// measure, so the image draws 1:1 in an unbounded parent. Pin an exact box with
/// `.size(w, h)` to scale it into a layout-owned box.
pub fn image(source: ImageSource) -> CustomDef {
    custom(ImagePrimitive { image: source })
}
