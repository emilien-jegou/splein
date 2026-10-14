// Single responsibility: Scoped builder context for emitting display list commands directly into chunks.

use crate::foundation::{Appearance, Color, Point, Radius, ResolvedRect, Transform};
use crate::scene::chunk::SceneChunk;
use crate::scene::command::SceneCommand;
use crate::scene::image::ImageSource;
use crate::scene::vector::VectorGraphic;
use crate::text::layout::TextLayout;
use std::sync::Arc;

/// Context provided to custom primitives to record display list commands directly into a chunk.
pub struct PaintContext<'a> {
    chunk: &'a mut SceneChunk,
}

impl<'a> PaintContext<'a> {
    /// Constructs a direct-to-chunk paint context.
    #[inline(always)]
    pub fn new(chunk: &'a mut SceneChunk) -> Self {
        Self { chunk }
    }

    /// Appends a rectangle fill or stroke command.
    pub fn draw_rect(&mut self, rect: ResolvedRect, appearance: Appearance) {
        self.chunk.push(SceneCommand::DrawRect { rect, appearance });
    }

    /// Appends a raster image drawing command.
    pub fn draw_image(&mut self, rect: ResolvedRect, image: ImageSource) {
        self.chunk.push(SceneCommand::DrawImage { rect, image });
    }

    /// Appends a vector graphic drawing command.
    pub fn draw_svg(&mut self, rect: ResolvedRect, graphic: VectorGraphic) {
        self.chunk.push(SceneCommand::DrawSvg { rect, graphic });
    }

    /// Appends a shaped typography run command.
    pub fn draw_text(&mut self, origin: Point, layout: Arc<TextLayout>, color: Color) {
        self.chunk.push(SceneCommand::DrawText {
            origin,
            layout,
            color,
        });
    }

    /// Scopes commands within a 2D affine transformation matrix.
    pub fn with_transform<F: FnOnce(&mut PaintContext)>(&mut self, t: Transform, f: F) {
        self.chunk.push(SceneCommand::PushTransform(t));
        f(self);
        self.chunk.push(SceneCommand::PopTransform);
    }

    /// Scopes commands within a rounded rectangular clip boundary.
    pub fn with_clip<F: FnOnce(&mut PaintContext)>(&mut self, rect: ResolvedRect, r: Radius, f: F) {
        self.chunk.push(SceneCommand::PushClip { rect, radius: r });
        f(self);
        self.chunk.push(SceneCommand::PopClip);
    }
}
