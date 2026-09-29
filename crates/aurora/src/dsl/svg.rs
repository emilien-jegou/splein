// Single responsibility: First-class declarative builder for vector graphics.

use crate::dsl::custom::{custom, CustomDef};
use crate::foundation::{Constraints, IntrinsicSize, ResolvedRect};
use crate::scene::context::PaintContext;
use crate::scene::vector::VectorGraphic;
use crate::tree::Primitive;
use std::sync::Arc;

struct SvgPrimitive {
    graphic: VectorGraphic,
    width: f32,
    height: f32,
}

impl Primitive for SvgPrimitive {
    fn measure(&self, _constraints: Constraints) -> IntrinsicSize {
        IntrinsicSize {
            width: self.width,
            height: self.height,
        }
    }

    fn paint(&self, ctx: &mut PaintContext, bounds: ResolvedRect) {
        ctx.draw_svg(bounds, self.graphic.clone());
    }
}

/// Constructs a first-class declarative SVG element.
pub fn svg(graphic: impl Into<VectorGraphic>) -> CustomDef {
    let g = graphic.into();
    let (w, h) = (g.width, g.height);
    custom(SvgPrimitive {
        graphic: g,
        width: w,
        height: h,
    })
    .size(w, h)
}

impl From<Arc<VectorGraphic>> for VectorGraphic {
    fn from(v: Arc<VectorGraphic>) -> Self {
        (*v).clone()
    }
}

impl From<&Arc<VectorGraphic>> for VectorGraphic {
    fn from(v: &Arc<VectorGraphic>) -> Self {
        (**v).clone()
    }
}
