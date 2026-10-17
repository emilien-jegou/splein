// Single responsibility: Node coordinate framing, transformation, and display list command emission.

use crate::foundation::{GlassLayer, Point, Radius, ResolvedRect, ShadowShape, Transform};
use crate::scene::{LayerId, PaintContext, SceneChunk, SceneCommand};
use crate::text::TextContext;
use crate::tree::LayoutNode;
use crate::tree::NodeKind;

/// Offscreen margin an alpha-silhouette drop shadow needs around its
/// layer's content, in layout px — largest over the shadow's layers of
/// `max(|offset|) + layer_pad` (the offset displaces the silhouette, the
/// pad covers the blur reach on top of it). `0.0` when the node casts no
/// alpha drop shadow (including `inset`, which paints as a box interior).
///
/// This is also the layer-existence test: a non-zero pad means the
/// element's content alpha feeds a shadow, so the layer needs an
/// offscreen even without a filter. [`node_needs_layer`] and the
/// `BeginLayer` emission both ask this one function, so "open a layer"
/// and "size its offscreen" can never disagree.
#[inline(always)]
pub(crate) fn node_alpha_shadow_pad(node: &LayoutNode) -> f32 {
    let Some(shadow) = &node.style.appearance.shadow else {
        return 0.0;
    };
    if shadow.shape != ShadowShape::Alpha
        || shadow.inset
        || shadow.layers().is_empty()
    {
        return 0.0;
    }
    let light = &node.state.core.resolved_light;
    shadow
        .layers()
        .iter()
        .map(|layer| {
            let (ox, oy) = shadow.layer_offset(light, layer);
            ox.abs().max(oy.abs()) + shadow.layer_pad(layer)
        })
        .fold(0.0, f32::max)
}

/// Whether a node opens a layer, and what glass that layer carries.
///
/// Decided in `hierarchy.rs` (it alone sees ancestors): a group root
/// always opens one, a glass node inside a group never does (its shape
/// joins the group's layer), everything else follows the old rule
/// (explicit `has_layer`, filter, own glass, alpha-silhouette shadow)
/// plus blend: any non-`Normal` blend mode opens one (the backdrop
/// snapshot + in-shader composite live in the layer).
/// One struct, passed to both enter and exit, so the push and pop can
/// never disagree.
#[derive(Clone, Debug)]
pub struct NodeLayer {
    pub needs_layer: bool,
    /// The `BeginLayer` rect: the content box, except group roots whose
    /// rect is the union of member rects (group-local, may start
    /// off-origin when members overhang).
    pub rect: ResolvedRect,
    /// `Single` (own material, radius stamped) or `Group` (collected
    /// members); `None` for glass-free layers.
    pub glass: Option<GlassLayer>,
    /// Press content transform (node-local, about the content centre):
    /// the pressed member's content scales with its lens. `None` when
    /// the press is glow/light only (identity transform).
    pub press: Option<Transform>,
}

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
    paint: Option<ResolvedRect>,
    layer: &NodeLayer,
) {
    // Clipping and layer bounds follow the render box; the content drawn inside keeps
    // its flow size (see `emit_node_content`), which is what makes a growing box reveal
    // static content instead of squashing it.
    let painted = paint.unwrap_or(node.resolved_rect);
    let local_rect = ResolvedRect::new(0.0, 0.0, painted.width, painted.height);
    chunk.push(SceneCommand::PushOffset(Point::new(x, y)));
    // A glass node's cast shadow paints to the PARENT target, *before* its
    // glass layer opens: inside the layer it would be rasterized into the
    // glass offscreen, which the lens composite replaces (the shader is
    // transparent outside the shape) — an invisible shadow. `emit_node_content`
    // skips the shadow when this hoist fires, so it draws exactly once.
    // Identity-transform glass (every shipped demo) is unaffected by the
    // hoist landing before the local transform below.
    if layer.glass.is_some() {
        if let Some(s) = &node.style.appearance.shadow {
            if !s.inset {
                chunk.push(SceneCommand::DrawShadow {
                    rect: local_rect,
                    radius: node.style.appearance.radius,
                    shadow: s.clone(),
                    light: node.state.core.resolved_light,
                });
            }
        }
    }
    if layer.needs_layer {
        chunk.push(SceneCommand::BeginLayer {
            id: layer_id,
            rect: layer.rect,
            filter: node.style.appearance.filter.clone(),
            glass: layer.glass.clone(),
            shadow_pad: node_alpha_shadow_pad(node),
            blend: node.style.appearance.blend,
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
    layer: &NodeLayer,
) {
    // Pops exactly what the matching enter pushed (same `NodeLayer`), so
    // the pop can never disagree with the push and unbalance the clip
    // stack.
    if node_clip_radius(node).is_some() {
        chunk.push(SceneCommand::PopClip);
    }
    if node.effective_opacity() < 1.0 {
        chunk.push(SceneCommand::PopOpacity);
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

/// Emits shadows, rect fills/strokes, shaped text, or custom primitives into the active chunk.
///
/// `caret_visible` is the engine's blink phase; the caret paints only while it
/// is on (and the node is focused with a collapsed selection).
/// `abs_origin_y` is the absolute layout-space Y of the node's content
/// origin (accumulated ancestor offsets). A resting text run nudges its
/// whole content (glyphs, decorations, caret) by one extra offset so the
/// absolute run Y lands on a whole pixel — engines rasterize whatever
/// origin they receive, no flag needed.
pub fn emit_node_content(
    node: &mut LayoutNode,
    chunk: &mut SceneChunk,
    text_ctx: &TextContext,
    caret_visible: bool,
    abs_origin_y: f32,
    paint: Option<ResolvedRect>,
    layer: &NodeLayer,
) {
    let settled = paint.unwrap_or(node.resolved_rect);
    // A node that declares itself a clipping boundary owns its render box as a *surface*:
    // the card the user sees growing must paint up to the edge that is visibly moving, not
    // stop at its settled height while its own contents reveal past it. A node that is only
    // being repositioned by a transition is not a boundary, so its background stays at the
    // flow size and only its origin moves.
    let surface_rect = if node.style.clip {
        ResolvedRect::new(0.0, 0.0, settled.width, settled.height)
    } else {
        ResolvedRect::new(0.0, 0.0, settled.width, settled.height)
    };
    // Interior content is always laid out at the flow size and revealed by the box above,
    // which is what stops glyphs from stretching while the surface grows around them.
    let flow_rect = ResolvedRect::new(0.0, 0.0, node.resolved_rect.width, node.resolved_rect.height);

    // Drop shadows paint behind the fill; inset shadows darken the fill's
    // interior and must paint after it (CSS: inset box-shadow sits above
    // the background, below content). A glass node's shadow was already
    // hoisted to the parent target in `enter_node_frame`, so it is skipped
    // here (inside the glass offscreen it would never be seen).
    if let Some(s) = &node.style.appearance.shadow {
        if !s.inset && layer.glass.is_none() {
            chunk.push(SceneCommand::DrawShadow {
                rect: surface_rect,
                radius: node.style.appearance.radius,
                shadow: s.clone(),
                light: node.state.core.resolved_light,
            });
        }
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

    if let Some(s) = &node.style.appearance.shadow {
        if s.inset {
            chunk.push(SceneCommand::DrawShadow {
                rect: surface_rect,
                radius: node.style.appearance.radius,
                shadow: s.clone(),
                light: node.state.core.resolved_light,
            });
        }
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
            // The node's own desired/displayed record decides the nudge:
            // moving follows exactly, resting glides to the whole pixel —
            // glyphs, decorations, selection, and caret all sit inside, so
            // they stay glued. X untouched.
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
