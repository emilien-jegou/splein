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
///
/// Sized by content: the intrinsic vector size is the `Fit` measure, so the graphic
/// draws at natural size in an unbounded parent and shrinks with its min-content floor
/// under pressure. Pin an exact box with `.size(w, h)` when the layout, not the
/// artwork, owns the dimensions.
pub fn svg(graphic: impl Into<VectorGraphic>) -> CustomDef {
    let g = graphic.into();
    custom(SvgPrimitive {
        width: g.width,
        height: g.height,
        graphic: g,
    })
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
