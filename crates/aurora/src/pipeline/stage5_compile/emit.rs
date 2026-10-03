// Single responsibility: Node coordinate framing, transformation, and display list command emission.

use crate::foundation::{Constraints, Point, ResolvedRect, Transform};
use crate::scene::{LayerId, PaintContext, SceneChunk, SceneCommand};
use crate::text::TextContext;
use crate::tree::{LayoutNode, NodeKind};

/// Pushes affine transforms, opacity, layers, and clips for a node into the chunk.
#[inline(always)]
pub fn enter_node_frame(
    node: &LayoutNode,
    chunk: &mut SceneChunk,
    x: f32,
    y: f32,
    layer_id: LayerId,
) {
    let local_rect = ResolvedRect::new(
        0.0,
        0.0,
        node.resolved_rect.width,
        node.resolved_rect.height,
    );
    chunk.push(SceneCommand::PushOffset(Point::new(x, y)));
    if node.style.has_layer {
        chunk.push(SceneCommand::BeginLayer {
            id: layer_id,
            rect: local_rect,
        });
    }
    if node.transform != Transform::IDENTITY {
        chunk.push(SceneCommand::PushTransform(node.transform));
    }
    if node.style.appearance.opacity < 1.0 {
        chunk.push(SceneCommand::PushOpacity(node.style.appearance.opacity));
    }
    if node.style.clip {
        chunk.push(SceneCommand::PushClip {
            rect: local_rect,
            radius: node.style.appearance.radius,
        });
    }
}

/// Pops active frames, clips, layers, and offsets in reverse order.
#[inline(always)]
pub fn exit_node_frame(node: &LayoutNode, chunk: &mut SceneChunk, layer_id: LayerId) {
    if node.style.clip {
        chunk.push(SceneCommand::PopClip);
    }
    if node.style.appearance.opacity < 1.0 {
        chunk.push(SceneCommand::PopOpacity);
    }
    if node.transform != Transform::IDENTITY {
        chunk.push(SceneCommand::PopTransform);
    }
    if node.style.has_layer {
        chunk.push(SceneCommand::EndLayer { id: layer_id });
    }
    chunk.push(SceneCommand::PopOffset);
}

/// Emits shadows, rect fills/strokes, shaped text, or custom primitives into the active chunk.
pub fn emit_node_content(node: &LayoutNode, chunk: &mut SceneChunk, text_ctx: &TextContext) {
    let local_rect = ResolvedRect::new(
        0.0,
        0.0,
        node.resolved_rect.width,
        node.resolved_rect.height,
    );

    for s in &node.style.appearance.shadows {
        chunk.push(SceneCommand::DrawShadow {
            rect: local_rect,
            radius: node.style.appearance.radius,
            shadow: *s,
        });
    }

    if node.style.appearance.fill.is_some() || node.style.appearance.stroke.is_some() {
        chunk.push(SceneCommand::DrawRect {
            rect: local_rect,
            appearance: node.style.appearance.clone(),
        });
    }

    match &node.kind {
        NodeKind::Text(t) => {
            // Reuse the measurement layout so wrapping and box height stay consistent.
            let layout = node.state.cached_text_layout.clone().unwrap_or_else(|| {
                let max_w = if node.style.width.is_fit() {
                    f32::INFINITY
                } else {
                    node.resolved_rect.width.max(1.0)
                };
                text_ctx.shape_config(t, Constraints::loose(max_w, f32::INFINITY))
            });
            chunk.push(SceneCommand::DrawText {
                origin: Point::ZERO,
                layout,
                color: t.color,
            });
        }
        NodeKind::Custom(p) => {
            p.paint(&mut PaintContext::new(chunk), local_rect);
        }
        NodeKind::Group => {}
    }
}
