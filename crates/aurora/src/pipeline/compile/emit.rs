// Single responsibility: Node coordinate framing, transformation, and display list command emission.

use crate::foundation::{Appearance, Constraints, Fill, Point, ResolvedRect, Transform};
use crate::scene::{LayerId, PaintContext, SceneChunk, SceneCommand};
use crate::text::decoration::decoration_rules;
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
    let transform = node.effective_transform();
    if transform != Transform::IDENTITY {
        chunk.push(SceneCommand::PushTransform(transform));
    }
    let opacity = node.effective_opacity();
    if opacity < 1.0 {
        chunk.push(SceneCommand::PushOpacity(opacity));
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
    if node.effective_opacity() < 1.0 {
        chunk.push(SceneCommand::PopOpacity);
    }
    if node.effective_transform() != Transform::IDENTITY {
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
        // The enclosing PushOpacity already carries opacity, so the brush must not apply it twice.
        let mut appearance = node.style.appearance.clone();
        if node.effective_opacity() < 1.0 {
            appearance.opacity = 1.0;
        }
        chunk.push(SceneCommand::DrawRect {
            rect: local_rect,
            appearance,
        });
    }

    match &node.kind {
        NodeKind::Text(t) => {
            // Alignment is baked into the shaped layout, so it is shared without mutation.
            let layout = node.state.cached_text_layout.clone().unwrap_or_else(|| {
                let max_w = if node.style.width.is_fit() {
                    f32::INFINITY
                } else {
                    node.resolved_rect.width.max(1.0)
                };
                text_ctx.shape_config(t, Constraints::loose(max_w, f32::INFINITY))
            });
            let decorations: Vec<ResolvedRect> = if t.decoration.is_empty() {
                Vec::new()
            } else {
                layout
                    .lines
                    .iter()
                    .flat_map(|line| decoration_rules(line, t.size, t.decoration))
                    .collect()
            };
            chunk.push(SceneCommand::DrawText {
                origin: Point::ZERO,
                layout,
                color: t.color,
            });
            for rect in decorations {
                chunk.push(SceneCommand::DrawRect {
                    rect,
                    appearance: Appearance::EMPTY.with_fill(Fill::Solid(t.color)),
                });
            }
        }
        NodeKind::Custom(p) => {
            p.paint(&mut PaintContext::new(chunk), local_rect);
        }
        NodeKind::Group => {}
    }
}
