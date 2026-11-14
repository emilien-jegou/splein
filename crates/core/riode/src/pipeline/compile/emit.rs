// Single responsibility: Node coordinate framing, transformation, and display list command emission.

use crate::foundation::{LayerPaint, Point, ResolvedRect, Transform};
use crate::scene::{LayerId, PaintContext, SceneChunk, SceneCommand};
use crate::text::TextContext;
use crate::tree::LayoutNode;
use crate::tree::NodeKind;

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
    let flow_rect = ResolvedRect::new(0.0, 0.0, node.resolved_rect.width, node.resolved_rect.height);

    // 1. Direct paint for leaf media primitives (Image / Custom)
    if node.kind.paint_leaf(&mut PaintContext::new(chunk), flow_rect) {
        return;
    }

    // 2. Text layout & emission
    if let NodeKind::Text(t) = &node.kind {
        let layout = t.resolve_paint_layout(
            text_ctx,
            node.state.text.cached_text_layout.clone(),
            node.style.width.is_fit(),
            (node.resolved_rect.width - node.style.padding.horizontal()).max(0.0),
        );
        let pad = node.style.padding;
        let run_y = abs_origin_y + pad.top;
        let adj = node.state.text.placement.observe(run_y, crate::text::baseline_snap_y());
        chunk.has_text = true;
        chunk.text_settled = chunk.text_settled && node.state.text.placement.is_settled();
        if pad.left != 0.0 || pad.top != 0.0 {
            chunk.push(SceneCommand::PushOffset(Point::new(pad.left, pad.top)));
        }
        if adj != 0.0 {
            chunk.push(SceneCommand::PushOffset(Point::new(0.0, adj)));
        }
        t.emit(
            &layout,
            chunk,
            layer_id,
            node.state.text.text_selection,
            node.state.input.text_focused && caret_visible,
        );
        if adj != 0.0 {
            chunk.push(SceneCommand::PopOffset);
        }
        if pad.left != 0.0 || pad.top != 0.0 {
            chunk.push(SceneCommand::PopOffset);
        }
    }
}
