// Single responsibility: Pure geometric damage analysis comparing visual bounds against history.

use crate::foundation::{DamageRegion, LightEnvironment, Point, ResolvedRect, Transform};
use crate::tree::TreeArena;
use crate::tree::{DirtyFlags, NodeId, NodeKind};


/// Damage calculation plan containing aggregated damaged regions and removed node bounds.
#[derive(Clone, Debug, Default)]
pub struct DamagePlan {
    /// Minimal non-overlapping damage rectangles.
    pub region: DamageRegion,
    /// Screen-space bounds of nodes unmounted this frame.
    pub removed_rects: Vec<ResolvedRect>,
}

impl DamagePlan {
    /// Computes damage regions, skipping un-mutated subtrees in O(M) when layout is clean.
    #[tracing::instrument(skip_all)]
    pub fn compute(
        arena: &mut TreeArena,
        root: NodeId,
        removed_bounds: Vec<ResolvedRect>,
        dirty_nodes: &[NodeId],
        laid_out: bool,
    ) -> Self {
        let mut plan = Self {
            region: DamageRegion::new(),
            removed_rects: removed_bounds,
        };

        for removed in &plan.removed_rects {
            plan.region.push(*removed);
        }

        // Fast Path 1: Idle frame (no nodes dirtied, no layout, no unmounted rects)
        if !laid_out && dirty_nodes.is_empty() && plan.removed_rects.is_empty() {
            return plan;
        }
        // Fast Path 2: Paint-only mutations walk only the dirty subtrees (O(M*K)), pushing
        // each node's previous and current bounds so a moved node damages both positions.
        //
        // The walk is clip-threaded: each dirty root starts from the clip its
        // ancestors establish (a scroll viewport's box included), so scrolled
        // or repainted content inside a clip never damages outside it.
        if !laid_out {
            for &id in dirty_nodes {
                if !arena.is_valid(id) {
                    continue;
                }
                let parent_tx = arena
                    .parent(id)
                    .map_or(Transform::IDENTITY, |p| arena.node_absolute_transform(p));
                let (root_clip, root_viewport) = ancestor_clipping(arena, id);
                resolve_spatial_and_damage(
                    arena,
                    id,
                    parent_tx,
                    root_clip,
                    root_viewport,
                    false,
                    LightEnvironment::DEFAULT,
                    &mut plan.region,
                );
            }
            expand_nested_shifts(&mut plan);
            return plan;
        }

        tracing::debug!(
            "aurora: damage plan laid_out={} dirty_nodes={} removed={}",
            laid_out,
            dirty_nodes.len(),
            plan.removed_rects.len(),
        );

        // Layout path: Full / boundary tree resolution with hierarchical pruning
        let root_node = arena.get(root);
        let root_clip = if root_node.style.clip {
            Some(ResolvedRect::new(
                0.0,
                0.0,
                root_node.resolved_rect.width,
                root_node.resolved_rect.height,
            ))
        } else {
            None
        };
        let root_viewport = if root_node.style.clip && root_node.style.scrollable {
            root_clip
        } else {
            None
        };

        resolve_spatial_and_damage(
            arena,
            root,
            Transform::IDENTITY,
            root_clip,
            root_viewport,
            false,
            LightEnvironment::DEFAULT,
            &mut plan.region,
        );
        expand_nested_shifts(&mut plan);
        plan
    }
}

/// Nearest scrollable+clip ancestor of `id`: the viewport whose box the
/// damage walk threaded down. `expected` is that box (recomputed here would
/// be identical); the walk up finds its node for overlay repair. `None` when
/// the threaded box came from nowhere searchable (defensive; skips repair).
fn scroll_viewport_node(
    arena: &TreeArena,
    id: NodeId,
    expected: Option<ResolvedRect>,
) -> Option<NodeId> {
    if expected.is_none() {
        return None;
    }
    let mut current = arena.parent(id);
    while let Some(parent) = current {
        let node = arena.get(parent);
        if node.style.clip && node.style.scrollable {
            return Some(parent);
        }
        current = arena.parent(parent);
    }
    None
}

/// Union of last-painted bounds over a subtree: where its pixels sat last
/// frame. Read-only mid-walk (commit runs post-frame), so it is stable no
/// matter the visit order. Nodes never painted contribute nothing — mount
/// damage already covers them.
fn subtree_old_bounds(arena: &TreeArena, id: NodeId) -> ResolvedRect {
    if !arena.is_valid(id) {
        return ResolvedRect::ZERO;
    }
    let mut out = arena
        .get(id)
        .state
        .paint
        .last_painted_bounds
        .unwrap_or(ResolvedRect::ZERO);
    for child in arena.each_child(id) {
        out = out.union(&subtree_old_bounds(arena, child));
    }
    out
}

/// Whether `node` is `ancestor` or descends from it.
fn is_within(arena: &TreeArena, mut node: NodeId, ancestor: NodeId) -> bool {
    loop {
        if node == ancestor {
            return true;
        }
        match arena.parent(node) {
            Some(p) => node = p,
            None => return false,
        }
    }
}

/// Repairs overlay pixels a scroll blit corrupts.
///
/// The blit moves every pixel in the viewport box, but overlays (thumb,
/// sticky — anything in the viewport outside the shifted content subtree)
/// do not move with content: their old pixels get copied to a wrong
/// destination, which no other damage covers. Pushing each overlay's old
/// bounds shifted by the delta repaints exactly those destinations from the
/// new scene. Overlays' own old/new positions stay covered by their own
/// damage (or are untouched sources, when static).
fn repair_scroll_overlays(
    arena: &TreeArena,
    shifted: NodeId,
    viewport: NodeId,
    delta: Point,
    damage: &mut DamageRegion,
) {
    if shifted == viewport {
        // The viewport itself moved: the whole subtree moves rigidly with it,
        // so the blit is correct everywhere inside.
        return;
    }
    // Direct child of the viewport on the shifted node's path — the one
    // subtree that moves with the shift and needs no repair.
    let mut top = shifted;
    while let Some(p) = arena.parent(top) {
        if p == viewport {
            break;
        }
        top = p;
    }
    for child in arena.each_child(viewport) {
        if child == top || is_within(arena, shifted, child) {
            continue;
        }
        let old = subtree_old_bounds(arena, child);
        if !old.is_empty() {
            damage.push(old.shifted_by(delta));
        }
    }
}

/// Expands nested same-tick shifts to full boxes.
///
/// Sequential blits compose, but overlay repair is computed pre-frame: when
/// one shift's clip contains another's, the inner repair coordinates go
/// stale under the outer blit. This is rare (two viewports scrolling one
/// frame); the outer box repaint is always correct. Single shifts — the
/// common case — keep the fast strip path untouched.
fn expand_nested_shifts(plan: &mut DamagePlan) {
    // Common case first: 0-1 shifts need no pairwise scan and no
    // allocation (the to_vec below exists only to end the borrow).
    if plan.region.scroll_shifts().len() < 2 {
        return;
    }
    let shifts = plan.region.scroll_shifts().to_vec();
    for (i, a) in shifts.iter().enumerate() {
        for (j, b) in shifts.iter().enumerate() {
            if i != j && (a.clip == b.clip || a.clip.contains_rect(&b.clip)) {
                plan.region.push(a.clip);
            }
        }
    }
}
///
/// A node's own clip is excluded from `clip` (it bounds the node's children,
/// not itself; the recursive walk threads it down via `next_clip`). The
/// viewport half exists for Fast Path 2 roots, where the walk starts mid-tree
/// and never passes the viewport node itself.
fn ancestor_clipping(arena: &TreeArena, id: NodeId) -> (Option<ResolvedRect>, Option<ResolvedRect>) {
    let mut clip: Option<ResolvedRect> = None;
    let mut viewport: Option<ResolvedRect> = None;
    let mut current = arena.parent(id);
    while let Some(parent) = current {
        let node = arena.get(parent);
        if node.style.clip {
            let origin = arena.node_absolute_point(parent);
            let box_rect = ResolvedRect::new(
                origin.x,
                origin.y,
                node.resolved_rect.width,
                node.resolved_rect.height,
            );
            clip = Some(clip.map_or(box_rect, |c| c.intersect(&box_rect)));
            // Innermost wins: ancestors overwrite only when nothing closer set.
            if viewport.is_none() && node.style.scrollable {
                viewport = Some(box_rect);
            }
        }
        current = arena.parent(parent);
    }
    (clip, viewport)
}

fn resolve_spatial_and_damage(
    arena: &mut TreeArena,
    id: NodeId,
    parent_tx: Transform,
    active_clip: Option<ResolvedRect>,
    scroll_viewport: Option<ResolvedRect>,
    ancestor_shifted: bool,
    inherited_light: LightEnvironment,
    damage: &mut DamageRegion,
) -> ResolvedRect {
    let (node_tx, is_clip, is_scroll_viewport, painted) = {
        let node = arena.get(id);
        // Origin comes from the render box so damage follows what is on screen.
        let painted = node.resolved_rect;
        let mut tx = parent_tx.multiply(&Transform::from_translation(painted.x, painted.y));
        let local_tx = node.effective_transform();
        if local_tx != Transform::IDENTITY {
            tx = tx.multiply(&local_tx);
        }
        (
            tx,
            node.style.clip,
            node.style.clip && node.style.scrollable,
            painted,
        )
    };

    // Ambient light resolves top-down: the nearest override wins, and the
    // resolved value is stashed for paint/bounds/hit testing to read. The
    // stash always precedes its consumers within a frame (damage runs before
    // compile), so it can never feed a stale light to fresh paint.
    let ambient = arena
        .get(id)
        .style
        .appearance
        .light_override
        .unwrap_or(inherited_light);
    arena.get_mut(id).state.core.resolved_light = ambient;

    let node_abs = node_tx.transform_point(Point::new(0.0, 0.0));
    // Unclipped occupancy drives damage: a node leaving its clip must still clear its old pixels.
    let full_bounds = arena.get(id).compute_visual_bounds(&node_tx, None);
    // Visible extent under the active clip: the unclipped box intersected.
    // A second full projection (transform + shadow loop) for the same box
    // is pure waste (N1) — intersect is a few comparisons.
    let visual_bounds = match active_clip {
        Some(clip) => full_bounds.intersect(&clip),
        None => full_bounds,
    };
    let mut subtree_bounds = visual_bounds;
    // Flow bounds are computed from the same render box with the same pads
    // (`bounds_sized` over `resolved_rect` == `compute_visual_bounds`
    // inputs): provably identical to `visual_bounds`, so copy instead of
    // re-projecting a third time (N1). The stored/compared flow history
    // keeps working — both sides of every comparison change together.
    let flow_bounds = visual_bounds;

    // [S2-2] Static-subtree freeze: a proven-static, clean subtree whose own
    // occupancy, flow, and resolved light are all identical to last frame
    // cannot contribute damage — and neither can its descendants, whose
    // positions derive from this node's transform plus their own unchanged
    // rects. Full-bounds equality is the complete proof: it folds the entire
    // ancestor chain (any ancestor move shifts these bounds and voids the
    // skip), and the ambient check folds the light chain (an ancestor light
    // change with no movement would otherwise leave stale stashed lights).
    // Children's *stored* subtree bounds are still current (same argument,
    // inductively), so the union below equals a full recompute.
    //
    // The memo + all child memos must be present: any missed invalidation
    // leaves `None` somewhere on the path and forces the full walk, which
    // recomputes the proofs bottom-up. Stale-`true` can never skip.
    let frozen = (|| {
        let node = arena.get(id);
        if node.state.core.subtree_static != Some(true) {
            return false;
        }
        if node.state.core.dirty.intersects(
            DirtyFlags::MEASURE
                | DirtyFlags::LAYOUT
                | DirtyFlags::PAINT
                | DirtyFlags::SCROLL
                | DirtyFlags::SUBTREE_DIRTY,
        ) {
            return false;
        }
        if node.state.paint.last_painted_bounds != Some(full_bounds) {
            return false;
        }
        if node.state.paint.last_painted_flow != Some(flow_bounds) {
            return false;
        }
        if node.state.core.resolved_light != ambient {
            return false;
        }
        for child in arena.each_child(id) {
            if arena.get(child).state.core.subtree_static != Some(true) {
                return false;
            }
        }
        true
    })();
    if frozen {
        for child in arena.each_child(id) {
            subtree_bounds = subtree_bounds.union(&arena.get(child).state.paint.subtree_bounds);
        }
        arena.get_mut(id).state.paint.subtree_bounds = subtree_bounds;
        return subtree_bounds;
    }

    // Scroll fast path, decided before descending: a transform-only
    // SCROLL-dirty node inside a scroll viewport moved by a pure
    // translation. Damage is the newly-exposed strip only (plus a 1px seam
    // pad for the blit's device rounding); the backend moves the covered
    // bulk via the recorded shift. Descendants inherit `shifted` as
    // `ancestor_shifted`: their old/new positions are already covered by
    // this shift, so they stay silent unless they changed themselves.
    let shifted = {
        let node = arena.get(id);
        let dirty = node.state.core.dirty;
        let transform_only = dirty.contains(DirtyFlags::SCROLL)
            && !dirty.contains(DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT);
        let old = node.state.paint.last_painted_bounds;
        match (transform_only, scroll_viewport, old) {
            (true, Some(viewport), Some(old)) => {
                let (dx, dy) = (full_bounds.x - old.x, full_bounds.y - old.y);
                if dx == 0.0 && dy == 0.0 {
                    false
                } else {
                    let covered = viewport.intersect(&viewport.shifted_by(Point::new(dx, dy)));
                    let mut exposed_any = false;
                    for strip in viewport.subtract(&covered) {
                        let padded = strip.expand(1.0).intersect(&viewport);
                        if !padded.is_empty() {
                            damage.push(padded);
                            exposed_any = true;
                        }
                    }
                    if exposed_any {
                        damage.push_scroll_shift(viewport, Point::new(dx, dy));
                        // The blit moves overlay pixels (thumb, sticky) that
                        // do not move with content — repair their destinations.
                        if let Some(vp) =
                            scroll_viewport_node(arena, id, scroll_viewport)
                        {
                            repair_scroll_overlays(arena, id, vp, Point::new(dx, dy), damage);
                        }
                    }
                    true
                }
            }
            _ => false,
        }
    };

    let next_clip = if is_clip {
        let box_rect = ResolvedRect::new(node_abs.x, node_abs.y, painted.width, painted.height);
        Some(active_clip.map_or(box_rect, |c| c.intersect(&box_rect)))
    } else {
        active_clip
    };
    // The viewport box threads down like the clip; a nested scroll viewport
    // overwrites it for its own subtree (contain: shifts never escape it).
    let next_viewport = if is_scroll_viewport {
        let box_rect = ResolvedRect::new(node_abs.x, node_abs.y, painted.width, painted.height);
        Some(box_rect)
    } else {
        scroll_viewport
    };

    let child_count = arena.children(id).len();
    for i in 0..child_count {
        let child = arena.children(id)[i];
        let child_subtree = resolve_spatial_and_damage(
            arena,
            child,
            node_tx,
            next_clip,
            next_viewport,
            ancestor_shifted || shifted,
            ambient,
            damage,
        );
        subtree_bounds = subtree_bounds.union(&child_subtree);
    }

    let node = arena.get_mut(id);
    let old_bounds = node.state.paint.last_painted_bounds;
    let old_flow = node.state.paint.last_painted_flow;
    let is_text_mutation = matches!(node.kind, NodeKind::Text(_))
        && (node.state.core.dirty.contains(DirtyFlags::MEASURE)
            || node.state.core.dirty.contains(DirtyFlags::LAYOUT));

    if let Some(old) = old_bounds {
        // Either the box moved or the interior re-flowed: both vacate pixels.
        //
        // Clipped to the active clip: paint outside it never happened (compile culls
        // there), so there is nothing to clear beyond it — including for a node that
        // moved, whose old painted pixels were inside the clip it was painted under.
        //
        // A node whose move an ancestor shift already covers stays silent
        // unless it changed itself (measure/layout/paint): the ancestor's
        // blit moved its old pixels and the strip covers the rest.
        let self_changed = node.state.core.dirty.contains(
            DirtyFlags::MEASURE | DirtyFlags::LAYOUT | DirtyFlags::PAINT,
        );
        if !shifted
            && (!ancestor_shifted || self_changed)
            && (old != full_bounds || old_flow != Some(flow_bounds))
        {
                match active_clip {
                Some(clip) => {
                    damage.push(old.intersect(&clip));
                    damage.push(full_bounds.intersect(&clip));
                }
                None => {
                    damage.push(old);
                    damage.push(full_bounds);
                }
            }
        // A shifted node needs no repaint on top: the blit moved the covered
        // bulk and the strip above covers the rest. Without this guard the
        // branch below re-adds the whole clipped subtree on every scroll tick.
        } else if !shifted
            && (node.state.core.dirty.contains(DirtyFlags::PAINT)
                || node.state.core.dirty.contains(DirtyFlags::SCROLL)
                || is_text_mutation
                || (ancestor_shifted && self_changed))
        {
            let damage_target = if child_count > 0 {
                subtree_bounds
            } else {
                full_bounds
            };
            // New paint never escapes the active clip (compile culls it there), so
            // clip it: a text caret blinking at a clipped edge must not damage
            // outside the clip.
            match active_clip {
                Some(clip) => damage.push(damage_target.intersect(&clip)),
                None => damage.push(damage_target),
            }
        }
    } else {
        damage.push(full_bounds);
    }

    // [S2-2] Bottom-up static proof (free: this walk already visited the
    // children, whose memos are current). Skipped subtrees keep their proven
    // memo; anything else recomputes here.
    let own_static = {
        let n = arena.get(id);
        n.state.core.bindings.is_static() && n.state.core.provided.is_none()
    };
    let mut proven = own_static;
    if proven {
        for child in arena.each_child(id) {
            if arena.get(child).state.core.subtree_static != Some(true) {
                proven = false;
                break;
            }
        }
    }
    let n = arena.get_mut(id);
    n.state.paint.subtree_bounds = subtree_bounds;
    n.state.core.subtree_static = Some(proven);
    subtree_bounds
}
