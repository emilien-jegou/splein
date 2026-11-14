// Single responsibility: First-class declarative builder for vector graphics and build-time rasterized SVGs.

use std::sync::Arc;
use crate::dsl::custom::{custom, CustomDef};
use crate::dsl::element::IntoElement;
use crate::dsl::image::{image, ImageElement};
use crate::foundation::{Constraints, IntrinsicSize, ResolvedRect};
use crate::scene::context::PaintContext;
use crate::scene::image::ImageSource;
use crate::scene::vector::VectorGraphic;
use crate::tree::Primitive;

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

pub trait IntoSvg {
    type Output: IntoElement;
    fn into_svg(self) -> Self::Output;
}

impl IntoSvg for VectorGraphic {
    type Output = CustomDef;
    fn into_svg(self) -> Self::Output {
        custom(SvgPrimitive {
            width: self.width,
            height: self.height,
            graphic: self,
        })
    }
}

impl IntoSvg for Arc<VectorGraphic> {
    type Output = CustomDef;
    fn into_svg(self) -> Self::Output {
        (*self).clone().into_svg()
    }
}

impl IntoSvg for &Arc<VectorGraphic> {
    type Output = CustomDef;
    fn into_svg(self) -> Self::Output {
        (**self).clone().into_svg()
    }
}

impl IntoSvg for ImageSource {
    type Output = ImageElement;
    fn into_svg(self) -> Self::Output {
        image(self)
    }
}

/// Constructs a first-class declarative SVG element.
pub fn svg<T: IntoSvg>(graphic: T) -> T::Output {
    graphic.into_svg()
}
