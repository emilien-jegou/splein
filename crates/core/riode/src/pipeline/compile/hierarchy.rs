// Single responsibility: Node tree visual bounds evaluation, framing, and command emission.

use crate::foundation::{
    Backdrop, BuiltinStage, EffectShader, Glass, GlassEffect, GlassGroup, GlassLayer,
    GlassMember, IntoEffect, GlassPressGeometry, Glow, Point, Refraction, ResolvedRect,
    Spectral, Transform, GLASS_STANDARD,
};
use crate::pipeline::compile::emit::{
    emit_node_content, enter_node_frame, exit_node_frame, node_alpha_shadow_pad, NodeLayer,
};
use crate::scene::{LayerId, SceneChunk};
use crate::text::TextContext;
use crate::tree::TreeArena;
use crate::tree::{DirtyFlags, NodeId};

/// Nearest ancestor (exclusive) carrying `glass_group`: the layer whose
/// shapes this node's glass joins, if any. Nested group markers are
/// transparent — members join the outermost group.
fn enclosing_glass_group(arena: &TreeArena, id: NodeId) -> Option<NodeId> {
    let mut current = arena.parent(id);
    while let Some(p) = current {
        if arena
            .get(p)
            .style
            .appearance
            .backdrop_effects
            .as_ref()
            .is_some_and(|s| s.glass_group_spacing().is_some())
        {
            return Some(p);
        }
        current = arena.parent(p);
    }
    None
}

/// Group-layer-local AABB of a `w × h` box through a general matrix, plus
/// the smaller axis scale (for the radius fallback when a member's path
/// is not translation-only).
fn skewed_bounds(m: &Transform, w: f32, h: f32) -> (ResolvedRect, f32) {
    let mut x0 = f32::INFINITY;
    let mut y0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    let mut y1 = f32::NEG_INFINITY;
    for (px, py) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
        let p = m.transform_point(Point::new(px, py));
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let sx = (m.a * m.a + m.b * m.b).sqrt();
    let sy = (m.c * m.c + m.d * m.d).sqrt();
    (
        ResolvedRect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)),
        sx.min(sy),
    )
}

/// Collects the glass members of a group root: every descendant carrying
/// a material contributes its rect in group-layer-local logical px with
/// its declared corners stamped.
///
/// Positions accumulate `resolved_rect` translations plus each node's own
/// effective translation — the same placement the traversal paints at,
/// minus the subpixel settle glide (shapes track the layout target
/// exactly; the AA fringe covers the rest difference). A scale/rotation
/// anywhere on the path falls back to an AABB silhouette with the radius
/// scaled by the smaller axis. Members with `glass_id` resolve through
/// the group's retained morph tracks, so the emitted rect glides toward
/// the layout target on the morph spring instead of snapping.
fn collect_glass_members(arena: &mut TreeArena, group_id: NodeId) -> (Vec<GlassMember>, Option<(Point, f32)>) {
    struct Frame {
        id: NodeId,
        matrix: Transform,
        skewed: bool,
    }
    let mut out: Vec<GlassMember> = Vec::new();
    // The pressed member (at most one press exists): index + geometry,
    // applied after the morph resolve below.
    let mut pressed: Option<(usize, GlassPressGeometry)> = None;
    let mut stack = vec![Frame {
        id: group_id,
        matrix: Transform::IDENTITY,
        skewed: false,
    }];
    while let Some(frame) = stack.pop() {
        for child in arena.children(frame.id) {
            let node = arena.get(child);
            let step = Transform::from_translation(node.resolved_rect.x, node.resolved_rect.y)
                .multiply(&node.effective_transform());
            let matrix = frame.matrix.multiply(&step);
            let skewed = frame.skewed || !node.effective_transform().is_translation();
            // A group marker never contributes its own material (a root's
            // is ignored by the layer decision; a nested marker is fully
            // transparent) — only members refract.
            let is_marker = node
                .style
                .appearance
                .backdrop_effects
                .as_ref()
                .is_some_and(|s| s.glass_group_spacing().is_some());
            if !is_marker {
                if let Some((mut material, member_backdrop, _, _, _)) = node
                    .style
                    .appearance
                    .backdrop_effects
                    .as_ref()
                    .and_then(|s| s.glass_single())
                {
                material.radius = node.style.appearance.radius;
                let (w, h) = (node.resolved_rect.width, node.resolved_rect.height);
                let (rect, rscale) = if skewed {
                    skewed_bounds(&matrix, w, h)
                } else {
                    // Pure translations: the matrix holds the exact
                    // offset, no corner-walk error.
                    (ResolvedRect::new(matrix.tx, matrix.ty, w, h), 1.0)
                };
                let mut member = GlassMember {
                    rect,
                    glass: material,
                    backdrop: member_backdrop,
                    light: 0.0,
                    radius_scale: 1.0,
                };
                if rscale != 1.0 {
                    member.glass.radius =
                        crate::foundation::Radius::Scalar(member.radius() * rscale);
                }
                out.push(member);
                // Press deforms the member's own shape (the content
                // transform rides along in the traversal below).
                if pressed.is_none() {
                    if let Some(press) = arena.get(child).state.glass_press.as_ref() {
                        if press.is_active() {
                            pressed =
                                Some((out.len() - 1, press.geometry(&GLASS_STANDARD.motion)));
                        }
                    }
                }
            }
            }
            stack.push(Frame {
                id: child,
                matrix,
                skewed,
            });
        }
    }
    if out.is_empty() {
        return (out, None);
    }
    // Morph glide: match layout targets against retained tracks by
    // `glass_id`, then overwrite each tracked member's rect + stamped
    // radius with the track's current glide position. Untracked members
    // keep their declared corners verbatim.
    let targets: Vec<(Option<u64>, ResolvedRect, f32)> = out
        .iter()
        .map(|m| (m.glass.glass_id, m.rect, m.radius()))
        .collect();
    let motion = &GLASS_STANDARD.motion;
    let current = arena
        .get_mut(group_id)
        .state
        .glass_morph
        .resolve(&targets, motion.morph_response, motion.morph_damping);
    for (member, ((id, _, _), (rect, radius))) in
        out.iter_mut().zip(targets.iter().zip(current.iter()))
    {
        if id.is_some() {
            member.rect = *rect;
            member.glass.radius = crate::foundation::Radius::Scalar(*radius);
        }
    }
    // Press deforms the morphed rect (morph first, press on top — the
    // reference composes them the same way): rect + radius scale about
    // the member centre, light on the shape, touch into frame coords.
    let mut press_touch: Option<(Point, f32)> = None;
    if let Some((idx, geo)) = pressed {
        if let Some(m) = out.get_mut(idx) {
            m.rect = geo.apply(m.rect);
            m.radius_scale = geo.radius_scale();
            m.light = geo.light;
            let t = geo
                .touch
                .unwrap_or(Point::new(m.rect.width / 2.0, m.rect.height / 2.0));
            press_touch = Some((
                Point::new(m.rect.x + t.x, m.rect.y + t.y),
                geo.glow,
            ));
        }
    }
    (out, press_touch)
}

/// Recursively traverses a node hierarchy and emits commands into the current scene chunk.
pub fn compile_node_hierarchy(
    arena: &mut TreeArena,
    id: NodeId,
    chunk: &mut SceneChunk,
    text_ctx: &TextContext,
    viewport: ResolvedRect,
    parent_tx: Transform,
    is_stacking_root: bool,
    caret_visible: bool,
) {
    let node = arena.get(id);
    // A stacking context root offsets its chunk from the inherited origin; flow nodes nest locally.
    // Every node is placed by its render box, so a node whose box is mid-flight lands
    // where it is being drawn rather than where layout eventually wants it.
    let painted = node.resolved_rect;
    // Glass-group layering (Phase 5): the nearest enclosing group wins.
    // A group root opens one shared layer; a glass node inside a group
    // contributes a shape instead of opening its own; a nested group
    // marker is transparent. Read off before any mutable borrow below.
    let enclosing = enclosing_glass_group(arena, id);
    // Glass is a backdrop-stage phenomenon (the lens refracts the
    // backdrop): detection reads the backdrop stack.
    let backdrop_stack = node.style.appearance.backdrop_effects.clone();
    let is_group_root = backdrop_stack
        .as_ref()
        .is_some_and(|s| s.glass_group_spacing().is_some())
        && enclosing.is_none();
    // A lone lens only self-renders outside any group and off a group
    // root: a member's shape joins the enclosing group's shared optics
    // instead of opening its own (base parity — see `collect_glass_members`).
    let own_glass = if enclosing.is_none() && !is_group_root {
        backdrop_stack.as_ref().and_then(|s| s.glass_single())
    } else {
        None
    };
    let node_has_layer = node.style.has_layer;
    let node_has_effects = node.style.appearance.backdrop_effects.is_some()
        || node.style.appearance.layer_effects.is_some();
    let node_glass_radius = node.style.appearance.radius;
    let glass_spacing = backdrop_stack
        .as_ref()
        .and_then(|s| s.glass_group_spacing());
    let group_clips = node.style.clip;
    let group_opacity = node.style.appearance.opacity;
    let group_size = (painted.width, painted.height);
    let own_tx = if is_stacking_root {
        parent_tx.multiply(&node.effective_transform())
    } else {
        parent_tx.multiply(&Transform::from_translation(painted.x, painted.y))
    };
    let visual_screen = node.compute_visual_bounds(&own_tx, None);
    // A culled node keeps its flags (the early return skips the clear at the
    // end of this function), so culling a dirty node lets its parent replay
    // content that never included it — and nothing re-emits it on reveal.
    // Only settled subtrees cull: a dirty offscreen node encodes (once,
    // cheap) and clears together with its parent, so reveal always replays
    // complete content.
    let dirty = node.state.core.dirty;
    let subtree_settled = !dirty.contains(DirtyFlags::SUBTREE_DIRTY)
        && !dirty.contains(
            DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT | DirtyFlags::SCROLL,
        );
    if !visual_screen.overlaps(&viewport)
        && !node.style.has_layer
        && !is_stacking_root
        && subtree_settled
    {
        return;
    }

    let (off_x, off_y) = if is_stacking_root {
        let origin = parent_tx.transform_point(Point::ZERO);
        (origin.x, origin.y)
    } else {
        (painted.x, painted.y)
    };
    // Absolute layout-space Y of the node's content origin: accumulated
    // offsets through the ancestor chain, so a transition moving an
    // ancestor reads as motion here. (Pure canvas-scroll shifts applied by
    // engines below compile are invisible to this — accepted: scroll keeps
    // subpixel placement.)
    let abs_origin = own_tx.transform_point(Point::ZERO);
    // The layer decision mirrors the old `node_needs_layer` rule, with
    // the group logic folded in (see `NodeLayer`). Reads happen on the
    // shared borrow first; the mutable borrows below are sequential, and
    // collection runs between them (it needs `&mut` for morph tracks).
    let shadow_pad = node_alpha_shadow_pad(arena.get(id));
    let needs_layer = is_group_root
        || node_has_layer
        || node_has_effects
        || own_glass.is_some()
        || shadow_pad > 0.0;
    let content_rect = ResolvedRect::new(0.0, 0.0, painted.width, painted.height);
    // Press on this node (single or member): the shape deforms in the
    // collection below (members) or in a group-of-one (singles), and the
    // content rides along via `NodeLayer.press`. Group markers never
    // carry press (their material is ignored) — belt and braces with
    // the router's marker skip.
    let pressed_geo: Option<GlassPressGeometry> = if is_group_root {
        None
    } else {
        arena
            .get(id)
            .state
            .glass_press
            .as_ref()
            .filter(|press| press.is_active())
            .map(|press| press.geometry(&GLASS_STANDARD.motion))
    };
    let press_tx = pressed_geo.as_ref().and_then(|geo| {
        let (w, h) = (painted.width, painted.height);
        let (cx, cy) = (w / 2.0, h / 2.0);
        let tx = Transform::from_translation(cx + geo.translation.x, cy + geo.translation.y)
            .multiply(&Transform::from_scale(geo.scale_x, geo.scale_y))
            .multiply(&Transform::from_translation(-cx, -cy));
        if tx == Transform::IDENTITY {
            None
        } else {
            Some(tx)
        }
    });
    let (group_spectral, group_glow, group_refraction) = backdrop_stack
        .as_ref()
        .and_then(|s| s.glass_single())
        .map(|(_, _, sp, gl, r)| (sp, gl, r))
        .unwrap_or((Spectral::NONE, Glow::NONE, Refraction::NONE));
    let (layer_rect, layer_glass) = if is_group_root {
        let (members, press_touch) = collect_glass_members(arena, id);
        let union = members.iter().fold(ResolvedRect::ZERO, |acc, m| {
            if acc.is_empty() {
                m.rect
            } else if m.rect.is_empty() {
                acc
            } else {
                acc.union(&m.rect)
            }
        });
        let spacing = glass_spacing.unwrap_or(0.0);
        (
            union,
            Some(GlassLayer::Group(GlassGroup {
                members,
                smoothing: spacing * GLASS_STANDARD.merge_factor,
                clip: group_clips
                    .then(|| ResolvedRect::new(0.0, 0.0, group_size.0, group_size.1)),
                opacity: group_opacity,
                press_touch,
                spectral: group_spectral,
                glow: group_glow,
                refraction: group_refraction,
            })),
        )
    } else if let Some((g0, b0, _s0, _gl0, _r0)) = own_glass {
        // The glass SDF follows the node's declared corners: stamp the
        // radius into the material copy so engines resolve it against
        // the layer rect without re-reading tree style.
        match (Some(g0), pressed_geo) {
            (Some(g), Some(geo)) => {
                // A pressed single rides the group path as a group of
                // one (same uniforms as Single plus the press extras):
                // smoothing 0 (no smin partner), opacity 1 (the single
                // path's hardcode), no clip (the lens never clips on the
                // single path).
                let mut material = g;
                material.radius = node_glass_radius;
                let rect = geo.apply(content_rect);
                let t = geo.touch.unwrap_or(Point::new(
                    content_rect.width / 2.0,
                    content_rect.height / 2.0,
                ));
                let member = GlassMember {
                    rect,
                    glass: material,
                    backdrop: b0,
                    light: geo.light,
                    radius_scale: geo.radius_scale(),
                };
                (
                    rect,
                    Some(GlassLayer::Group(GlassGroup {
                        members: vec![member],
                        smoothing: 0.0,
                        clip: None,
                        opacity: 1.0,
                        press_touch: Some((
                            Point::new(rect.x + t.x, rect.y + t.y),
                            geo.glow,
                        )),
                        spectral: group_spectral,
                        glow: group_glow,
                        refraction: group_refraction,
                    })),
                )
            }
            (Some(mut g), None) => {
                g.radius = node_glass_radius;
                let (_, b, s, gl, r) = own_glass.unwrap();
                (content_rect, Some(GlassLayer::Single(g, b, s, gl, r)))
            }
            _ => (content_rect, None),
        }
    } else {
        (content_rect, None)
    };
    let layer = NodeLayer {
        needs_layer,
        rect: layer_rect,
        // The sheet: `layer_effects` only — glass never lives here (a
        // transparent sheet has nothing to refract).
        layer_effects: arena.get(id).style.appearance.layer_effects.clone(),
        // The backdrop side, carrying the resolved glass layer (group
        // or single) so the engine snapshot/optics lowering reads one
        // place. A root's own lens is ignored (members refract); the
        // group's shared spectral/glow/refraction ride the root's own
        // glass stage when present, else neutral.
        backdrop_effects: ({
            let mut stack = backdrop_stack.clone().unwrap_or_default();
            // Members of a group don't self-render their lens (it joins
            // the group's shared optics — base parity); strip Glass
            // stages from a member's own backdrop stack.
            if enclosing.is_some() && !is_group_root {
                stack.0.retain(|e| {
                    !matches!(
                        e.shader(),
                        EffectShader::Builtin(BuiltinStage::Glass(_))
                    )
                });
            }
            // Units without a lens: stamp a default material so the
            // fold has one Glass item to ride — the snapshot, the pack
            // and the islands all key off it. Group members never
            // self-render (their units stay inert). A Custom-bearing
            // stack (a material-DSL surface) likewise gets the neutral
            // geometry host (D-M4): the surface's custom stage shades
            // the card the host produces, so the host must exist and
            // run first.
            let has_unit = stack.0.iter().any(|e| {
                matches!(e.shader(), EffectShader::Builtin(s) if !s.glass_stage_ids().is_empty())
            });
            let has_custom = stack
                .0
                .iter()
                .any(|e| matches!(e.shader(), EffectShader::Custom(_)));
            let layer_glass = if layer_glass.is_none()
                && !(enclosing.is_some() && !is_group_root)
                && !stack.0.iter().any(|e| {
                    matches!(e.shader(), EffectShader::Builtin(BuiltinStage::Glass(_)))
                })
                && (has_unit || has_custom)
            {
                Some(GlassLayer::Single(
                    Glass::default(),
                    Backdrop::NONE,
                    Spectral::NONE,
                    Glow::NONE,
                    Refraction::NONE,
                ))
            } else {
                layer_glass
            };
            if let Some(gl) = layer_glass {
                let fx = GlassEffect::new(gl);
                // In place: the resolved lens (stamped radius, press
                // geometry, merged group) takes the position the author
                // gave the glass — appending would force glass-last and
                // destroy stack order.
                if let Some(pos) = stack.0.iter().position(|e| {
                    matches!(
                        e.shader(),
                        EffectShader::Builtin(BuiltinStage::Glass(_))
                    )
                }) {
                    stack.0[pos] = fx.into_effect();
                } else if let Some(mpos) = stack.0.iter().position(|e| {
                    matches!(
                        e.shader(),
                        EffectShader::Builtin(BuiltinStage::GlassGroup { .. })
                    )
                }) {
                    // Group roots carry the marker, not a lens: the
                    // merged group takes the marker's position.
                    stack.0[mpos] = fx.into_effect();
                } else if has_custom {
                    // Auto-stamped neutral host for a custom-only stack:
                    // the custom stage is a real pass that must run AFTER
                    // the host produces the card, so insert at the front
                    // (a unit-item stack instead folds its units INTO the
                    // glass pass, so append-order there is inert).
                    stack.0.insert(0, fx.into_effect());
                } else {
                    stack.push(fx);
                }
            }
            if stack.0.is_empty() {
                None
            } else {
                Some(stack)
            }
        }),
        press: press_tx,
    };
    // Glass motion never marks dirty (it integrates in the damage pass),
    // so a replaying chunk would show the stale stream forever. Flag the
    // chunk while a press deforms this node or its group morphs, and the
    // replay check below re-emits instead. Cleared on settle by the next
    // bake (fresh chunks start settled).
    if pressed_geo.is_some()
        || (is_group_root && arena.get(id).state.glass_morph.is_active())
    {
        chunk.glass_unsettled = true;
    }
    {
        let node_mut = arena.get_mut(id);
        let paint = node_mut.rect_settle.observe(node_mut.resolved_rect);
        let dx = paint.x - node_mut.resolved_rect.x;
        let dy = paint.y - node_mut.resolved_rect.y;
        enter_node_frame(
            node_mut,
            chunk,
            off_x + dx,
            off_y + dy,
            LayerId::from_packed(id.packed()),
            is_stacking_root,
            Some(paint),
            &layer,
        );
        emit_node_content(node_mut, chunk, text_ctx, caret_visible, abs_origin.y, Some(paint), &layer);
    }
    // Hoisted single copy (see bounds.rs): per-index `children()` calls
    // rebuild the whole child list N+1 times per node per frame.
    let kids = arena.children(id);
    let child_count = kids.len();
    // Children that live in their own chunks — real stacking contexts, or
    // isolated children of an `isolate_children` parent — are compiled by the
    // context loops, never inline here. Emitting both would double-paint.
    let isolates = arena.get(id).style.isolate_children;
    for i in 0..child_count {
        let child_id = kids[i];
        let child = arena.get(child_id);
        let creates_context = child.creates_stacking_context();
        if !creates_context && !isolates {
            compile_node_hierarchy(
                arena,
                child_id,
                chunk,
                text_ctx,
                viewport,
                own_tx,
                false,
                caret_visible,
            );
        }
    }

    let node_mut = arena.get_mut(id);
    exit_node_frame(
        node_mut,
        chunk,
        LayerId::from_packed(id.packed()),
        is_stacking_root,
        &layer,
    );
    node_mut
        .state
        .core
        .dirty
        .remove(DirtyFlags::PAINT | DirtyFlags::SCROLL | DirtyFlags::SUBTREE_DIRTY);
}
