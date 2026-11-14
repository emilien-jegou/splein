// Single responsibility: Node kind taxonomy, polymorphic leaf contracts, and intrinsic dispatch.

use std::sync::Arc;
use crate::foundation::{Constraints, Direction, ImageFit, IntrinsicSize, ResolvedRect};
use crate::scene::context::PaintContext;
use crate::scene::image::ImageSource;
use crate::text::layout::TextLayout;
use crate::text::{TextConfig, TextContext};

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

pub trait Primitive {
    fn measure(&self, constraints: Constraints) -> IntrinsicSize;
    fn min_content(&self, axis: Direction) -> f32 {
        let size = self.measure(Constraints::UNCONSTRAINED);
        if axis == Direction::Horizontal { size.width } else { size.height }
    }
    fn paint(&self, ctx: &mut PaintContext, bounds: ResolvedRect);
}

#[derive(Default)]
pub enum NodeKind {
    #[default]
    Group,
    Text(TextConfig),
    Image(ImageConfig),
    Custom(std::rc::Rc<dyn Primitive>),
}

impl NodeKind {
    #[inline(always)]
    pub fn is_group(&self) -> bool {
        matches!(self, Self::Group)
    }

    #[inline(always)]
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    #[inline(always)]
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image(_))
    }

    pub fn text_content(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(&t.content),
            _ => None,
        }
    }

    pub fn is_selectable(&self) -> bool {
        match self {
            Self::Text(t) => t.selectable,
            _ => false,
        }
    }

    pub fn measure_leaf(
        &self,
        constraints: Constraints,
        text_ctx: &TextContext,
    ) -> Option<(IntrinsicSize, Option<Arc<TextLayout>>)> {
        match self {
            Self::Text(t) => {
                let (size, layout) = t.measure(text_ctx, constraints);
                Some((size, Some(layout)))
            }
            Self::Image(img) => Some((img.measure(constraints), None)),
            Self::Custom(p) => Some((p.measure(constraints), None)),
            Self::Group => None,
        }
    }

    pub fn min_content(&self, axis: Direction, text_ctx: &TextContext) -> Option<f32> {
        match self {
            Self::Text(t) => Some(t.min_content(text_ctx, axis)),
            Self::Image(img) => Some(img.min_content(axis)),
            Self::Custom(p) => Some(p.min_content(axis)),
            Self::Group => None,
        }
    }

    pub fn paint_leaf(&self, ctx: &mut PaintContext, bounds: ResolvedRect) -> bool {
        match self {
            Self::Image(img) => {
                img.paint(ctx, bounds);
                true
            }
            Self::Custom(p) => {
                p.paint(ctx, bounds);
                true
            }
            _ => false,
        }
    }
}
