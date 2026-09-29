// Single responsibility: Backend-agnostic display list drawing commands.

use crate::foundation::{Appearance, Color, Point, Radius, ResolvedRect, Shadow, Transform};
use crate::scene::image::ImageSource;
use crate::scene::vector::VectorGraphic;
use crate::text::layout::TextLayout;
use crate::tree::NodeId;

/// Stable identifier for cached layer-backed display list subtrees.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

impl From<NodeId> for LayerId {
    #[inline(always)]
    fn from(id: NodeId) -> Self {
        Self(((id.generation as u64) << 32) | (id.index as u64))
    }
}

/// Abstract display list drawing and state transformation commands.
#[derive(Clone, Debug)]
pub enum SceneCommand {
    PushTransform(Transform),
    PopTransform,
    PushOffset(Point),
    PopOffset,
    PushClip { rect: ResolvedRect, radius: Radius },
    PopClip,
    PushOpacity(f32),
    PopOpacity,
    BeginLayer { id: LayerId, rect: ResolvedRect },
    EndLayer { id: LayerId },
    DrawRect { rect: ResolvedRect, appearance: Appearance },
    DrawImage { rect: ResolvedRect, image: ImageSource },
    DrawSvg { rect: ResolvedRect, graphic: VectorGraphic },
    DrawShadow { rect: ResolvedRect, radius: Radius, shadow: Shadow },
    DrawText { origin: Point, layout: TextLayout, color: Color },
}
