// Single responsibility: Backend-agnostic display list drawing commands.

use crate::foundation::{Appearance, Color, Point, Radius, ResolvedRect, Shadow, Transform};
use crate::scene::image::ImageSource;
use crate::scene::vector::VectorGraphic;
use crate::text::layout::TextLayout;
use std::sync::Arc;

/// Stable identifier for cached layer-backed display list subtrees.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

impl LayerId {
    /// Wraps a packed generational node identity as a layer cache key.
    #[inline(always)]
    pub const fn from_packed(packed: u64) -> Self {
        Self(packed)
    }
}

/// Abstract display list drawing and state transformation commands.
#[derive(Clone, Debug)]
pub enum SceneCommand {
    PushTransform(Transform),
    PopTransform,
    PushOffset(Point),
    PopOffset,
    PushClip {
        rect: ResolvedRect,
        radius: Radius,
    },
    PopClip,
    PushOpacity(f32),
    PopOpacity,
    BeginLayer {
        id: LayerId,
        rect: ResolvedRect,
    },
    EndLayer {
        id: LayerId,
    },
    DrawRect {
        rect: ResolvedRect,
        appearance: Appearance,
    },
    DrawImage {
        rect: ResolvedRect,
        image: ImageSource,
    },
    DrawSvg {
        rect: ResolvedRect,
        graphic: VectorGraphic,
    },
    DrawShadow {
        rect: ResolvedRect,
        radius: Radius,
        shadow: Shadow,
    },
    DrawText {
        origin: Point,
        layout: Arc<TextLayout>,
        color: Color,
    },
}
