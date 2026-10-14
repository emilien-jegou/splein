// Single responsibility: Node coordinate framing, transformation, and display list command emission.

use crate::foundation::{Point, Radius, ResolvedRect, Transform};
use crate::scene::{LayerId, PaintContext, SceneChunk, SceneCommand};
use crate::text::TextContext;
use crate::tree::LayoutNode;
use crate::tree::NodeKind;

/// The corner radius of the single clip a node imposes on its content, or `None` when it clips
/// nothing at all.
///
/// One clip, not two. A node whose render box is animating is clipped to that box with *square*
/// corners: the box's edge is a moving reveal boundary, not a designed corner, so clipping to the
/// declared radius there would slice glyphs along a curve mid-flight. Once the box settles the
/// declared radius takes over and content is clipped by the real corners again.
///
/// Pushing both used to look equivalent but was not: the rounded clip and the square reveal bound
/// the same rect, so the intersection was the rounded shape and the square reveal was inert. Every
/// animating clipped view was therefore revealing along its corner curve.
///
/// This is the only place that rule is written down. The display-list push, the inherited-clip set a
/// descendant chunk has to re-assert, and the matching pop all consult it, because when they each
/// spelled the condition out they drifted -- and a push/pop pair that disagrees corrupts the clip
/// stack for every tile drawn after it, which is close to untraceable from a screenshot.
///
/// The rect is not part of the answer: callers pair this radius with the node's render box in
/// whichever space they need, local for the display list and absolute for inherited clips.
#[inline(always)]
pub fn node_clip_radius(node: &LayoutNode) -> Option<Radius> {
    if node.style.clip {
        Some(node.style.appearance.radius)
    } else {
        None
    }
}

/// Pushes affine transforms, opacity, layers, and clips for a node into the chunk.
///
/// When `hoist_root_transform` is set (stacking-context roots only), the node's
/// own effective transform is recorded into `chunk.placement` instead of being
/// pushed: placement is what moves when the node animates, so keeping it out
/// of the stream keeps content reusable across transform-only motion.
#[inline(always)]
pub fn enter_node_frame(
    node: &LayoutNode,
    chunk: &mut SceneChunk,
    x: f32,
    y: f32,
    layer_id: LayerId,
    hoist_root_transform: bool,
) {
    // Clipping and layer bounds follow the render box; the content drawn inside keeps
    // its flow size (see `emit_node_content`), which is what makes a growing box reveal
    // static content instead of squashing it.
    let painted = node.resolved_rect;
    let local_rect = ResolvedRect::new(0.0, 0.0, painted.width, painted.height);
    chunk.push(SceneCommand::PushOffset(Point::new(x, y)));
    if node.style.has_layer {
        chunk.push(SceneCommand::BeginLayer {
            id: layer_id,
            rect: local_rect,
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
    let opacity = node.effective_opacity();
    if opacity < 1.0 {
        chunk.push(SceneCommand::PushOpacity(opacity));
    }
    if let Some(radius) = node_clip_radius(node) {
        chunk.push(SceneCommand::PushClip {
            rect: local_rect,
            radius,
        });
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
) {
    // Asks the same authority `enter_node_frame` asked, so the pop can never disagree with the
    // push and unbalance the clip stack.
    if node_clip_radius(node).is_some() {
        chunk.push(SceneCommand::PopClip);
    }
    if node.effective_opacity() < 1.0 {
        chunk.push(SceneCommand::PopOpacity);
    }
    if node.effective_transform() != Transform::IDENTITY && !hoist_root_transform {
        chunk.push(SceneCommand::PopTransform);
    }
    if node.style.has_layer {
        chunk.push(SceneCommand::EndLayer { id: layer_id });
    }
    chunk.push(SceneCommand::PopOffset);
}

/// Emits shadows, rect fills/strokes, shaped text, or custom primitives into the active chunk.
///
/// `caret_visible` is the engine's blink phase; the caret paints only while it
/// is on (and the node is focused with a collapsed selection).
pub fn emit_node_content(
    node: &LayoutNode,
    chunk: &mut SceneChunk,
    text_ctx: &TextContext,
    caret_visible: bool,
) {
    // A node that declares itself a clipping boundary owns its render box as a *surface*:
    // the card the user sees growing must paint up to the edge that is visibly moving, not
    // stop at its settled height while its own contents reveal past it. A node that is only
    // being repositioned by a transition is not a boundary, so its background stays at the
    // flow size and only its origin moves.
    let surface_rect = if node.style.clip {
        let painted = node.resolved_rect;
        ResolvedRect::new(0.0, 0.0, painted.width, painted.height)
    } else {
        ResolvedRect::new(
            0.0,
            0.0,
            node.resolved_rect.width,
            node.resolved_rect.height,
        )
    };
    // Interior content is always laid out at the flow size and revealed by the box above,
    // which is what stops glyphs from stretching while the surface grows around them.
    let flow_rect = ResolvedRect::new(
        0.0,
        0.0,
        node.resolved_rect.width,
        node.resolved_rect.height,
    );

    if let Some(s) = &node.style.appearance.shadow {
        chunk.push(SceneCommand::DrawShadow {
            rect: surface_rect,
            radius: node.style.appearance.radius,
            shadow: s.clone(),
            light: node.state.core.resolved_light,
        });
    }

    if node.style.appearance.fill.is_some() || node.style.appearance.stroke.is_some() {
        // The enclosing PushOpacity already carries opacity, so the brush must not apply it twice.
        let mut appearance = node.style.appearance.clone();
        if node.effective_opacity() < 1.0 {
            appearance.opacity = 1.0;
        }
        chunk.push(SceneCommand::DrawRect {
            rect: surface_rect,
            appearance,
        });
    }

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
            let pad = node.style.padding;
            if pad.left != 0.0 || pad.top != 0.0 {
                chunk.push(SceneCommand::PushOffset(Point::new(pad.left, pad.top)));
            }
            t.emit(
                &layout,
                chunk,
                node.state.text.text_selection,
                node.state.input.text_focused && caret_visible,
            );
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
