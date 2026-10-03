// Single responsibility: Discrete bounded slice of display list commands.

use crate::foundation::{Point, Radius, ResolvedRect};
use crate::scene::command::{LayerId, SceneCommand};
use smallvec::SmallVec;

/// A clip rectangle in absolute screen space, with the radius it was declared with.
///
/// Radius survives as authored rather than being folded into the rect because the
/// renderer builds the mask path from both.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct InheritedClip {
    pub rect: ResolvedRect,
    pub radius: Radius,
}

/// Clips a chunk inherits from ancestors, already expressed in absolute screen space.
///
/// A chunk is replayed on its own, so an ancestor clip is not "still active" when the
/// chunk's commands run. These are re-pushed at the top of every chunk whose subtree
/// crosses a chunk boundary, which is what keeps a clipped parent's contents clipped.
pub type InheritedClips = SmallVec<[InheritedClip; 2]>;

/// A discrete, bounded slice of drawing commands belonging to a layout boundary or layer.
#[derive(Clone, Debug, Default)]
pub struct SceneChunk {
    pub bounds: ResolvedRect,
    pub abs_origin: Point,
    pub commands: Vec<SceneCommand>,
    pub layer_id: Option<LayerId>,
    /// Ancestor clips baked into this chunk's command list, kept for cache validation.
    pub inherited_clips: InheritedClips,
}

impl SceneChunk {
    pub fn new(bounds: ResolvedRect, abs_origin: Point) -> Self {
        Self {
            bounds,
            abs_origin,
            commands: Vec::new(),
            layer_id: None,
            inherited_clips: InheritedClips::new(),
        }
    }

    pub fn with_layer(bounds: ResolvedRect, abs_origin: Point, layer_id: LayerId) -> Self {
        Self {
            bounds,
            abs_origin,
            commands: Vec::new(),
            layer_id: Some(layer_id),
            inherited_clips: InheritedClips::new(),
        }
    }

    #[inline(always)]
    pub fn push(&mut self, cmd: SceneCommand) {
        self.commands.push(cmd);
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}
