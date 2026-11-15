// Single responsibility: Scoped builder context for emitting display list commands directly into chunks.

use crate::foundation::{Color, Point, ResolvedRect, Transform};
use crate::scene::chunk::SceneChunk;
use crate::scene::command::{LayerId, SceneCommand};
use crate::scene::image::ImageSource;
use crate::scene::vector::VectorGraphic;
use crate::text::layout::TextLayout;
use crate::text::TextContext;
use std::sync::Arc;

/// Context provided to primitives to record display list commands directly into a chunk.
///
/// A custom primitive paints images, vector graphics, or shaped text — anything
/// with its own raster path. Frame inputs (text shaping context, layer identity,
/// caret blink, absolute Y origin) come along so a text leaf can paint without
/// the compile pass knowing anything about text.
pub struct PaintContext<'a> {
    chunk: &'a mut SceneChunk,
    text: Option<&'a TextContext>,
    layer_id: LayerId,
    caret_visible: bool,
    abs_origin_y: f32,
}

impl<'a> PaintContext<'a> {
    /// Constructs a direct-to-chunk paint context (no frame inputs).
    #[inline(always)]
    pub fn new(chunk: &'a mut SceneChunk) -> Self {
        Self {
            chunk,
            text: None,
            layer_id: LayerId::from_packed(0),
            caret_visible: false,
            abs_origin_y: 0.0,
        }
    }

    /// Constructs the compile-pass paint context with every frame input a
    /// leaf's paint implementation may need.
    #[inline(always)]
    pub fn for_frame(
        chunk: &'a mut SceneChunk,
        text: &'a TextContext,
        layer_id: LayerId,
        caret_visible: bool,
        abs_origin_y: f32,
    ) -> Self {
        Self {
            chunk,
            text: Some(text),
            layer_id,
            caret_visible,
            abs_origin_y,
        }
    }

    /// Raw command push, for leaves with command sequences no helper covers.
    #[inline(always)]
    pub fn command(&mut self, command: SceneCommand) {
        self.chunk.push(command);
    }

    /// Marks the chunk as carrying shaped text (drives settle/idle accounting).
    #[inline(always)]
    pub fn mark_has_text(&mut self) {
        self.chunk.has_text = true;
    }

    /// Conjunction-writes the chunk's settled flag: text stays unsettled while
    /// any run is still gliding.
    #[inline(always)]
    pub fn and_text_settled(&mut self, settled: bool) {
        self.chunk.text_settled = self.chunk.text_settled && settled;
    }

    /// The shared text shaping context, when the frame provides one.
    #[inline(always)]
    pub fn text(&self) -> &'a TextContext {
        self.text.expect("paint context has no text context; only frame-scoped paints shape text")
    }

    /// The display layer identity this node paints into.
    #[inline(always)]
    pub fn layer_id(&self) -> LayerId {
        self.layer_id
    }

    /// Whether the caret blink is currently in its visible phase.
    #[inline(always)]
    pub fn caret_visible(&self) -> bool {
        self.caret_visible
    }

    /// Absolute Y of the node's box origin, for baseline settle bookkeeping.
    #[inline(always)]
    pub fn abs_origin_y(&self) -> f32 {
        self.abs_origin_y
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

    /// Scopes commands within a rectangular clip boundary (square — `radius`
    /// was purged).
    pub fn with_clip<F: FnOnce(&mut PaintContext)>(&mut self, rect: ResolvedRect, f: F) {
        self.chunk.push(SceneCommand::PushClip { rect });
        f(self);
        self.chunk.push(SceneCommand::PopClip);
    }
}
