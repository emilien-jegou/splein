// Single responsibility: Discrete bounded slice of display list commands.

use crate::foundation::{Point, ResolvedRect};
use crate::scene::command::{LayerId, SceneCommand};

/// A discrete, bounded slice of drawing commands belonging to a layout boundary or layer.
#[derive(Clone, Debug, Default)]
pub struct SceneChunk {
    pub bounds: ResolvedRect,
    pub abs_origin: Point,
    pub commands: Vec<SceneCommand>,
    pub layer_id: Option<LayerId>,
}

impl SceneChunk {
    pub fn new(bounds: ResolvedRect, abs_origin: Point) -> Self {
        Self {
            bounds,
            abs_origin,
            commands: Vec::new(),
            layer_id: None,
        }
    }

    pub fn with_layer(bounds: ResolvedRect, abs_origin: Point, layer_id: LayerId) -> Self {
        Self {
            bounds,
            abs_origin,
            commands: Vec::new(),
            layer_id: Some(layer_id),
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
