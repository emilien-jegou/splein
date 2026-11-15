// Single responsibility: Node coordinate framing, transformation, and display list command emission.

use crate::foundation::{LayerPaint, Point, ResolvedRect, Transform};
use crate::scene::{LayerId, PaintContext, SceneChunk, SceneCommand};
use crate::text::TextContext;
use crate::tree::LayoutNode;

#[derive(Clone, Debug)]
pub struct NodeLayer {
    pub needs_layer: bool,
    pub rect: ResolvedRect,
    pub paint: LayerPaint,
}

#[inline(always)]
pub fn enter_node_frame(
    node: &LayoutNode,
    chunk: &mut SceneChunk,
    x: f32,
    y: f32,
    layer_id: LayerId,
    hoist_root_transform: bool,
    paint: Option<ResolvedRect>,
    layer: &NodeLayer,
) {
    let painted = paint.unwrap_or(node.resolved_rect);
    let local_rect = ResolvedRect::new(0.0, 0.0, painted.width, painted.height);
    chunk.push(SceneCommand::PushOffset(Point::new(x, y)));
    if layer.needs_layer {
        chunk.push(SceneCommand::BeginLayer {
            id: layer_id,
            rect: layer.rect,
            paint: layer.paint.clone(),
        });
    }
    let transform = node.effective_transform();
    if transform != Transform::IDENTITY {
        if hoist_root_transform {
            chunk.placement = transform;
        } else {
            chunk.push(SceneCommand::PushTransform(transform));
        }
    }
    if node.style.clip {
        chunk.push(SceneCommand::PushClip { rect: local_rect });
    }
}

#[inline(always)]
pub fn exit_node_frame(
    node: &LayoutNode,
    chunk: &mut SceneChunk,
    layer_id: LayerId,
    hoist_root_transform: bool,
    layer: &NodeLayer,
) {
    if node.style.clip {
        chunk.push(SceneCommand::PopClip);
    }
    if node.effective_transform() != Transform::IDENTITY && !hoist_root_transform {
        chunk.push(SceneCommand::PopTransform);
    }
    if layer.needs_layer {
        chunk.push(SceneCommand::EndLayer { id: layer_id });
    }
    chunk.push(SceneCommand::PopOffset);
}

pub fn emit_node_content(
    node: &mut LayoutNode,
    chunk: &mut SceneChunk,
    text_ctx: &TextContext,
    layer_id: LayerId,
    caret_visible: bool,
    abs_origin_y: f32,
    paint: Option<ResolvedRect>,
    _layer: &NodeLayer,
) {
    let _ = paint;
    if !node.kind.is_leaf() {
        return;
    }
    let bounds = ResolvedRect::new(0.0, 0.0, node.resolved_rect.width, node.resolved_rect.height);
    let mut ctx = PaintContext::for_frame(chunk, text_ctx, layer_id, caret_visible, abs_origin_y);
    let kind = &node.kind;
    let style = node.style.clone();
    let state = &mut node.state;
    if let Some(primitive) = kind.primitive() {
        primitive.paint(&style, state, &mut ctx, bounds);
    }
}
