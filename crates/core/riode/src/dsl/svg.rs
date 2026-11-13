// Single responsibility: First-class declarative builder for vector graphics and build-time rasterized SVGs.

use std::sync::Arc;
use crate::dsl::custom::{custom, CustomDef};
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
    fn into_custom_def(self) -> CustomDef;
}

impl IntoSvg for VectorGraphic {
    fn into_custom_def(self) -> CustomDef {
        custom(SvgPrimitive {
            width: self.width,
            height: self.height,
            graphic: self,
        })
    }
}

impl IntoSvg for Arc<VectorGraphic> {
    fn into_custom_def(self) -> CustomDef {
        (*self).clone().into_custom_def()
    }
}

impl IntoSvg for &Arc<VectorGraphic> {
    fn into_custom_def(self) -> CustomDef {
        (**self).clone().into_custom_def()
    }
}

#[cfg(feature = "svg-runtime")]
impl IntoSvg for &str {
    fn into_custom_def(self) -> CustomDef {
        match VectorGraphic::from_str(self) {
            Ok(g) => g.into_custom_def(),
            Err(e) => {
                eprintln!("warning: failed to parse SVG string: {e}");
                VectorGraphic::stub(0.0, 0.0).into_custom_def()
            }
        }
    }
}

impl IntoSvg for ImageSource {
    fn into_custom_def(self) -> CustomDef {
        crate::dsl::image::image(self)
    }
}

/// Constructs a first-class declarative SVG element.
///
/// Accepts a runtime [`VectorGraphic`] or a build-time pre-rasterized [`ImageSource`]
/// produced by `include_svg!`.
pub fn svg(graphic: impl IntoSvg) -> CustomDef {
    graphic.into_custom_def()
}
