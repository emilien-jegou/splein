// Single responsibility: Declarative image leaf data — source pixels and object-fit policy.

use riode_core::foundation::{Constraints, Direction, ImageFit, IntrinsicSize, ResolvedRect};
use riode_core::scene::context::PaintContext;
use riode_core::scene::image::ImageSource;
use riode_core::tree::kind::{LeafChange, MeasureCtx, Primitive};
use riode_core::tree::{DirtyFlags, NodeState, NodeStyle};
use std::any::Any;

/// Declarative image leaf data: source pixels and object-fit policy.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageConfig {
    pub image: ImageSource,
    pub fit: ImageFit,
}

impl ImageConfig {
    pub fn measure(&self, constraints: Constraints) -> IntrinsicSize {
        let (iw, ih) = (self.image.width as f32, self.image.height as f32);
        if iw <= 0.0 || ih <= 0.0 {
            return IntrinsicSize { width: 0.0, height: 0.0 };
        }
        let ratio = iw / ih;
        if constraints.max_width.is_finite() && constraints.max_width > 0.0 {
            let w = constraints.max_width;
            return IntrinsicSize { width: w, height: w / ratio };
        }
        IntrinsicSize { width: iw, height: ih }
    }

    pub fn min_content(&self, axis: Direction) -> f32 {
        let (iw, ih) = (self.image.width as f32, self.image.height as f32);
        if axis == Direction::Horizontal { iw } else { ih }
    }

    pub fn paint(&self, ctx: &mut PaintContext, bounds: ResolvedRect) {
        let (iw, ih) = (self.image.width as f32, self.image.height as f32);
        if iw <= 0.0 || ih <= 0.0 || bounds.width <= 0.0 || bounds.height <= 0.0 {
            return;
        }

        let (draw_rect, needs_clip) = match self.fit {
            ImageFit::Fill => (bounds, false),
            ImageFit::Cover => {
                let scale = (bounds.width / iw).max(bounds.height / ih);
                let (w, h) = (iw * scale, ih * scale);
                (ResolvedRect::new(bounds.x, bounds.y, w, h), true)
            }
            ImageFit::CenterCrop => {
                let scale = (bounds.width / iw).max(bounds.height / ih);
                let (w, h) = (iw * scale, ih * scale);
                let x = bounds.x + (bounds.width - w) * 0.5;
                let y = bounds.y + (bounds.height - h) * 0.5;
                (ResolvedRect::new(x, y, w, h), true)
            }
            ImageFit::Contain => {
                let scale = (bounds.width / iw).min(bounds.height / ih);
                let (w, h) = (iw * scale, ih * scale);
                let x = bounds.x + (bounds.width - w) * 0.5;
                let y = bounds.y + (bounds.height - h) * 0.5;
                (ResolvedRect::new(x, y, w, h), false)
            }
            ImageFit::None => (ResolvedRect::new(bounds.x, bounds.y, iw, ih), true),
        };

        if needs_clip {
            ctx.with_clip(bounds, |c| {
                c.draw_image(draw_rect, self.image.clone());
            });
        } else {
            ctx.draw_image(draw_rect, self.image.clone());
        }
    }
}

impl Primitive for ImageConfig {
    fn measure(&self, _style: &NodeStyle, _state: &mut NodeState, ctx: &MeasureCtx) -> IntrinsicSize {
        self.measure(ctx.constraints)
    }

    fn min_content(&self, _style: &NodeStyle, axis: Direction, _text: &riode_core::text::TextContext) -> f32 {
        ImageConfig::min_content(self, axis)
    }

    fn paint(&self, _style: &NodeStyle, _state: &mut NodeState, ctx: &mut PaintContext, bounds: ResolvedRect) {
        ImageConfig::paint(self, ctx, bounds);
    }

    fn reconcile(&self, _next: &dyn Primitive) -> Option<LeafChange> {
        Some(LeafChange { dirty: DirtyFlags::MEASURE | DirtyFlags::LAYOUT, ..LeafChange::default() })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
