// Single responsibility: Scoped overlay recording surface for application extensions.

use aurora_foundation::{Point, ResolvedRect};
use crate::scene::{Scene, SceneChunk};

/// Appends extension-drawn overlay chunks to the frame display list.
pub struct OverlayPainter<'s> {
    /// Frame display list the recorded chunks are appended to.
    scene: &'s mut Scene,
}

impl<'s> OverlayPainter<'s> {
    /// Constructs a painter appending chunks to the given display list.
    #[inline(always)]
    pub fn new(scene: &'s mut Scene) -> Self {
        Self { scene }
    }

    /// Records one overlay chunk bounded by `bounds`, discarding it when empty.
    #[inline]
    /// The scene being overlaid, for a caller that needs to inspect it rather than draw.
    pub fn scene(&self) -> &Scene {
        self.scene
    }

    pub fn record(&mut self, bounds: ResolvedRect, draw: impl FnOnce(&mut SceneChunk)) {
        let mut chunk = SceneChunk::new(bounds, Point::ZERO);
        draw(&mut chunk);
        if !chunk.is_empty() {
            self.scene.push_chunk(chunk);
        }
    }
}
