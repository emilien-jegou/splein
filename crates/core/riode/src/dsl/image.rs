// Single responsibility: First-class declarative builder for raster images.

use crate::dsl::custom::{custom, CustomDef};
use crate::foundation::{Constraints, IntrinsicSize, ResolvedRect};
use crate::scene::context::PaintContext;
use crate::scene::image::ImageSource;
use crate::tree::Primitive;

struct ImagePrimitive {
    image: ImageSource,
}

impl Primitive for ImagePrimitive {
    fn measure(&self, constraints: Constraints) -> IntrinsicSize {
        let (iw, ih) = (self.image.width as f32, self.image.height as f32);
        if iw <= 0.0 || ih <= 0.0 {
            return IntrinsicSize { width: 0.0, height: 0.0 };
        }
        let ratio = iw / ih;

        // If a width constraint is offered, scale height by aspect ratio
        if constraints.max_width.is_finite() && constraints.max_width > 0.0 {
            let w = constraints.max_width;
            let h = (w / ratio).min(constraints.max_height);
            return IntrinsicSize { width: w, height: h };
        }

        IntrinsicSize { width: iw, height: ih }
    }

    fn paint(&self, ctx: &mut PaintContext, bounds: ResolvedRect) {
        ctx.draw_image(bounds, self.image.clone());
    }
}

/// Constructs a first-class declarative image element.
pub fn image(source: ImageSource) -> CustomDef {
    let (w, h) = (source.width as f32, source.height as f32);
    let mut def = custom(ImagePrimitive { image: source });
    if w > 0.0 && h > 0.0 {
        def.aspect_ratio = Some(w / h);
    }
    def
}
