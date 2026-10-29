// Single responsibility: Node coordinate framing, transformation, and display list command emission.

use crate::foundation::{Material, Point, ResolvedRect, Transform};
use crate::scene::{LayerId, PaintContext, SceneChunk, SceneCommand};
use crate::text::TextContext;
use crate::tree::LayoutNode;
use crate::tree::NodeKind;

/// Whether a node opens a layer, and what material that layer carries.
///
/// Decided in `hierarchy.rs` (it alone sees ancestors): a group root
/// always opens one, and any node carrying a `material` chain opens one
/// because the pass needs an offscreen to sample its backdrop into.
/// One struct, passed to both enter and exit, so the push and pop can
/// never disagree.
#[derive(Clone, Debug)]
pub struct NodeLayer {
    pub needs_layer: bool,
    /// The `BeginLayer` rect: the content box, except group roots whose
    /// rect is the union of member rects (group-local, may start
    /// off-origin when members overhang).
    pub rect: ResolvedRect,
    /// The material chain — the sole carrier of visual identity. `None`
    /// when the node declares no `material()`.
    pub material: Option<Material>,
    /// Press content transform (node-local, about the content centre):
    /// the pressed member's content scales with its lens. `None` when the
    /// press is glow/light only (identity transform).
    pub press: Option<Transform>,
}

impl NodeLayer {
    /// Whether the emitted layer runs the material pass: any material
    /// chain is the card now. The parent-target shadow hoist that used
    /// to key off this is gone with `DrawShadow`.
    pub fn has_glass(&self) -> bool {
        self.material.is_some()
    }

    /// Whether the emitted layer samples the backdrop: a material
    /// chain always does — the card refracts the sampled background.
    pub fn needs_backdrop(&self) -> bool {
        self.material.is_some()
    }
}

/// Pushes affine transforms, layers, and clips for a node into the chunk.
///
/// When `hoist_root_transform` is set (stacking-context roots only), the node's
/// own effective transform is recorded into `chunk.placement` instead of being
/// pushed: placement is what moves when the node animates, so keeping it out
/// of the stream keeps content reusable across transform-only motion.
///
/// Clips are square: `radius` was purged, so a clipping node bounds its
/// content with its render box outright.
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
    // Clipping and layer bounds follow the render box; the content drawn inside keeps
    // its flow size (see `emit_node_content`), which is what makes a growing box reveal
    // static content instead of squashing it.
    let painted = paint.unwrap_or(node.resolved_rect);
    let local_rect = ResolvedRect::new(0.0, 0.0, painted.width, painted.height);
    chunk.push(SceneCommand::PushOffset(Point::new(x, y)));
    if layer.needs_layer {
        chunk.push(SceneCommand::BeginLayer {
            id: layer_id,
            rect: layer.rect,
            material: layer.material.clone(),
        });
    }
    let transform = node.effective_transform();
    if transform != Transform::IDENTITY {
        if hoist_root_transform {
            // Placement, not content: the consumer applies it around the stream.
            chunk.placement = transform;
        } else {
            chunk.push(SceneCommand::PushTransform(transform));
        }
    }
    // Press content transform: the pressed member's content scales about
    // its centre with its lens (the reference's `contentTransform`).
    // Pushed in-stream (never hoisted): it is per-frame animation, not
    // reusable placement.
    let pressed = layer.press;
    if let Some(press) = pressed {
        chunk.push(SceneCommand::PushTransform(press));
    }
    if node.style.clip {
        chunk.push(SceneCommand::PushClip { rect: local_rect });
    }
}

/// Pops active frames, clips, layers, and offsets in reverse order.
///
/// Mirrors `enter_node_frame`'s hoist decision so the pop can never disagree
/// with the push.
#[inline(always)]
pub fn exit_node_frame(
    node: &LayoutNode,
    chunk: &mut SceneChunk,
    layer_id: LayerId,
    hoist_root_transform: bool,
    layer: &NodeLayer,
) {
    // Pops exactly what the matching enter pushed (same `NodeLayer`), so
    // the pop can never disagree with the push and unbalance the clip
    // stack.
    if node.style.clip {
        chunk.push(SceneCommand::PopClip);
    }
    if layer.press.is_some() {
        chunk.push(SceneCommand::PopTransform);
    }
    if node.effective_transform() != Transform::IDENTITY && !hoist_root_transform {
        chunk.push(SceneCommand::PopTransform);
    }
    if layer.needs_layer {
        chunk.push(SceneCommand::EndLayer { id: layer_id });
    }
    chunk.push(SceneCommand::PopOffset);
}

/// Emits shaped text or custom primitives into the active chunk.
///
/// There is no rect/shadow emission left: a group paints through its
/// material layer (opened in `enter_node_frame`), not through a draw
/// command here.
///
/// `caret_visible` is the engine's blink phase; the caret itself was
/// purged with the other internal visuals, so the flag only reaches
/// `Text::emit` for compatibility.
/// `abs_origin_y` is the absolute layout-space Y of the node's content
/// origin (accumulated ancestor offsets). A resting text run nudges its
/// whole content (glyphs, decorations) by one extra offset so the
/// absolute run Y lands on a whole pixel — engines rasterize whatever
/// origin they receive, no flag needed.
pub fn emit_node_content(
    node: &mut LayoutNode,
    chunk: &mut SceneChunk,
    text_ctx: &TextContext,
    caret_visible: bool,
    abs_origin_y: f32,
    paint: Option<ResolvedRect>,
    _layer: &NodeLayer,
) {
    let _ = paint;
    // Interior content is always laid out at the flow size and revealed by the box above,
    // which is what stops glyphs from stretching while the surface grows around them.
    let flow_rect = ResolvedRect::new(0.0, 0.0, node.resolved_rect.width, node.resolved_rect.height);

    match &node.kind {
        NodeKind::Text(t) => {
            let layout = t.resolve_paint_layout(
                text_ctx,
                node.state.text.cached_text_layout.clone(),
                node.style.width.is_fit(),
                (node.resolved_rect.width - node.style.padding.horizontal()).max(0.0),
            );
            // Text inks at the padding origin: the box grows around the run,
            // and the run starts inset rather than at the border edge.
            // The node's own desired/displayed record decides the nudge:
            // moving follows exactly, resting glides to the whole pixel —
            // glyphs sit inside, so they stay glued. X untouched.
            let pad = node.style.padding;
            let run_y = abs_origin_y + pad.top;
            let adj = node
                .state
                .text
                .placement
                .observe(run_y, crate::text::baseline_snap_y());
            // Chunk coherence for the replay path: the offset above is
            // baked into this stream, so a cached replay is only valid
            // while every text run in here sits settled.
            chunk.has_text = true;
            chunk.text_settled =
                chunk.text_settled && node.state.text.placement.is_settled();
            if pad.left != 0.0 || pad.top != 0.0 {
                chunk.push(SceneCommand::PushOffset(Point::new(pad.left, pad.top)));
            }
            if adj != 0.0 {
                chunk.push(SceneCommand::PushOffset(Point::new(0.0, adj)));
            }
            t.emit(
                &layout,
                chunk,
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
        NodeKind::Custom(p) => {
            p.paint(&mut PaintContext::new(chunk), flow_rect);
        }
        NodeKind::Group => {}
    }
}
