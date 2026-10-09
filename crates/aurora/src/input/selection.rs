// Single responsibility: Readonly text selection gestures: press, drag-extend, release,
// keyboard selection, and clipboard reads.

use aurora_foundation::Point;
use aurora_text::selection::{
    caret_at_point, expand_selection, snap_to_boundary, word_bounds, SelectionGranularity,
    TextSelection,
};
use crate::tree::{DirtyFlags, NodeId, NodeKind};
use super::router::PointerRouter;
use crate::input::event::PointerEvent;
use crate::tree::TreeArena;
/// Native selection gesture state: the fixed press edge (anchor) and the
/// moving edge (focus), each as a node plus a byte offset, with the
/// click-count granularity latched on press.
///
/// Same-node ranges expand by granularity (word/paragraph); once the focus
/// crosses onto another run the edges downgrade to character precision
/// (word presses snap only their own edge to the word), and every run
/// between anchor and focus in document order paints fully covered.
#[derive(Clone, Debug)]
pub(crate) struct ActiveSelection {
    pub(crate) anchor_node: NodeId,
    pub(crate) anchor: usize,
    pub(crate) focus_node: NodeId,
    pub(crate) focus: usize,
    pub(crate) granularity: SelectionGranularity,
    /// Runs currently painted for this gesture, so runs that leave the range
    /// repaint clean.
    pub(crate) involved: Vec<NodeId>,
}

impl ActiveSelection {
    fn single(node: NodeId, anchor: usize, granularity: SelectionGranularity) -> Self {
        Self {
            anchor_node: node,
            anchor,
            focus_node: node,
            focus: anchor,
            granularity,
            involved: Vec::new(),
        }
    }

    fn is_single(&self) -> bool {
        self.anchor_node == self.focus_node
    }
}

/// A press on selectable text whose layout wasn't shaped yet.
#[derive(Copy, Clone, Debug)]
pub(crate) struct PendingPress {
    node: NodeId,
    granularity: SelectionGranularity,
}

impl PointerRouter {
    /// Handles the selection half of a press on (or off) selectable text.
    pub(crate) fn press_selection(&mut self, arena: &mut TreeArena, event: &PointerEvent) {
        // A new press starts a new gesture description for tracing.
        self.last_extend_trace = None;        let target = self
            .press_chain
            .first()
            .copied()
            .filter(|id| selectable_text(arena, *id));
        let Some(node) = target else {
            // Pressed a non-selectable node: an existing highlight elsewhere
            // clears, but one on an ancestor of nothing in particular stays
            // simple — browsers clear on any foreign press.
            tracing::debug!(
                position = ?event.position,
                "press hit no selectable text; clearing selection"
            );
            self.clear_active_selection(arena);
            self.pending_collapse = None;
            self.pending_press = None;
            self.set_focus(arena, None);
            return;
        };
        // Empty runs take no caret, no highlight, and no focus: there is
        // nothing to show a selection over (F1).
        if text_is_empty(arena, node) {
            self.clear_active_selection(arena);
            self.pending_collapse = None;
            self.pending_press = None;
            self.set_focus(arena, None);
            return;
        }
        // Latch granularity from completed clicks in the burst: the second
        // Down already selects the word, the third the paragraph.
        let granularity = match self.tracker.burst_count() {
            0 => SelectionGranularity::Character,
            1 => SelectionGranularity::Word,
            _ => SelectionGranularity::Paragraph,
        };
        let Some(caret) = caret_for(arena, node, event.position) else {
            // No shaped layout cached yet (press before first paint): keep
            // focus, drop stale gesture state, and resolve the anchor lazily
            // from the press origin on the next drag step (F2).
            tracing::debug!(
                node = ?arena.get(node).key,
                "press with no shaped layout; deferring anchor"
            );
            self.set_focus(arena, Some(node));
            self.clear_active_selection_except(arena, node);
            self.pending_collapse = None;
            self.pending_press = Some(PendingPress { node, granularity });
            self.active_selection = None;
            return;
        };
        self.pending_press = None;
        // A press focuses for keyboard editing even before any caret resolves.
        self.set_focus(arena, Some(node));
        // A shift-press extends from the existing anchor (F6), across runs
        // when the stored anchor lives on another node (F4). Prefer the live
        // gesture anchor, but fall back to the stored range — and then to the
        // focused run's range — so Shift-click after the gesture ended still
        // extends instead of collapsing.
        if event.shift {
            let anchored = self
                .active_selection
                .as_ref()
                .filter(|a| arena.is_valid(a.anchor_node))
                .map(|a| (a.anchor_node, a.anchor, a.granularity))
                .or_else(|| {
                    self.focused
                        .filter(|f| *f != node)
                        .and_then(|f| current_selection(arena, f).map(|s| (f, s.anchor, granularity)))
                })
                .or_else(|| current_selection(arena, node).map(|s| (node, s.anchor, granularity)));
            if let Some((anchor_node, anchor, gran)) = anchored {
                let root = self.root;
                let prev = self
                    .active_selection
                    .as_ref()
                    .map(|a| a.involved.clone())
                    .unwrap_or_default();
                tracing::debug!(
                    anchor_node = ?arena.get(anchor_node).key,
                    anchor,
                    focus_node = ?arena.get(node).key,
                    focus = caret,
                    ?gran,
                    "shift-press extends selection"
                );
                let active = ActiveSelection {
                    anchor_node,
                    anchor,
                    focus_node: node,
                    focus: caret,
                    granularity: gran,
                    involved: Vec::new(),
                };
                let involved = paint_cross_selection(arena, root, &active, &prev);
                self.pending_collapse = None;
                self.active_selection = Some(ActiveSelection { involved, ..active });
                return;
            }
        }
        // Clear any highlight on another run first (all painted runs, F4).
        let prev = self
            .active_selection
            .as_ref()
            .map(|a| a.involved.clone())
            .unwrap_or_default();
        self.clear_active_selection_except(arena, node);
        // A fresh (single-click) press inside the current range defers the
        // collapse to release, so a drag from inside still starts fresh from
        // the press point. Word/paragraph presses always apply immediately:
        // the third press of a triple-click lands inside the double-click
        // word, and deferring it would keep the word instead of growing.
        if granularity == SelectionGranularity::Character {
            if let Some(current) = current_selection(arena, node) {
                let (s, e) = current.normalized();
                if !current.is_collapsed() && caret > s && caret < e {
                    tracing::debug!(
                        node = ?arena.get(node).key,
                        caret,
                        "press inside range; collapse deferred to release"
                    );
                    self.pending_collapse = Some((node, caret));
                    // The old range stays painted until release; keep its
                    // runs tracked so a drag from here repaints leavers.
                    self.active_selection = Some(ActiveSelection {
                        anchor_node: node,
                        anchor: caret,
                        focus_node: node,
                        focus: caret,
                        granularity,
                        involved: prev,
                    });
                    return;
                }
            }
        }
        let root = self.root;
        let active = ActiveSelection::single(node, caret, granularity);
        let involved = paint_cross_selection(arena, root, &active, &prev);
        tracing::debug!(
            node = ?arena.get(node).key,
            caret,
            ?granularity,
            "press applied selection"
        );
        self.pending_collapse = None;
        self.active_selection = Some(ActiveSelection { involved, ..active });
    }

    /// Extends the active selection toward the pointer during a drag.
    ///
    /// Cross-node (F4): the focus follows the pointer onto whatever selectable
    /// run is on top; every run between anchor and focus in document order
    /// paints its sub-range. Over a gap with no selectable run the focus stays
    /// and clamps onto its current run's edge, like a browser.
    pub(crate) fn extend_active_selection(&mut self, arena: &mut TreeArena, position: Point) {
        // Resolve a press that arrived before first paint (F2): the anchor
        // comes from the press origin once a layout exists.
        if self.active_selection.is_none() {
            if let Some(pending) = self.pending_press {
                if !arena.is_valid(pending.node) || !selectable_text(arena, pending.node) {
                    self.pending_press = None;
                } else if self.tracker.capture().is_some_and(|c| c != pending.node) {
                    self.pending_press = None;
                } else {
                    let origin = self.tracker.press_origin();
                    if let Some(anchor) = caret_for(arena, pending.node, origin) {
                        if let Some(caret) = caret_for(arena, pending.node, position) {
                            self.pending_press = None;
                            let root = self.root;
                            let active = ActiveSelection {
                                anchor_node: pending.node,
                                anchor,
                                focus_node: pending.node,
                                focus: caret,
                                granularity: pending.granularity,
                                involved: Vec::new(),
                            };
                            let involved =
                                paint_cross_selection(arena, root, &active, &[]);
                            self.active_selection = Some(ActiveSelection { involved, ..active });
                            return;
                        }
                    }
                }
            }
        }
        // Sweep-in (F3): a void press (no capture) that moves over selectable
        // text latches a fresh anchor at the entry point. Drags owned by a
        // handler-bearing node never hijack here.
        if self.active_selection.is_none() && self.tracker.capture().is_none() {
            if let Some(node) = self
                .chain
                .iter()
                .map(|h| h.id)
                .filter(|id| arena.is_valid(*id) && selectable_text(arena, *id))
                .find(|id| !text_is_empty(arena, *id))
            {
                if let Some(caret) = caret_for(arena, node, position) {
                    let granularity = SelectionGranularity::Character;
                    tracing::debug!(
                        node = ?arena.get(node).key,
                        caret,
                        "sweep-in latched selection"
                    );
                    self.tracker.set_capture(Some(node));
                    self.set_focus(arena, Some(node));
                    let root = self.root;
                    let active = ActiveSelection::single(node, caret, granularity);
                    let involved = paint_cross_selection(arena, root, &active, &[]);
                    self.active_selection = Some(ActiveSelection { involved, ..active });
                    return;
                }
            }
            return;
        }
        let Some(active) = self.active_selection.clone() else {
            return;
        };
        if !arena.is_valid(active.anchor_node) {
            self.active_selection = None;
            return;
        }
        // Only the gesture owner (the anchor run's capture) extends; other
        // drags leave it alone. The focus is free to roam onto other runs.
        if self.tracker.capture().is_some_and(|c| c != active.anchor_node) {
            return;
        }
        // The focus run is whatever selectable text is on top now; over a gap
        // it stays on its current run and clamps to the edge.
        let focus_node = self
            .chain
            .iter()
            .map(|h| h.id)
            .filter(|id| arena.is_valid(*id) && selectable_text(arena, *id))
            .find(|id| !text_is_empty(arena, *id))
            .unwrap_or(active.focus_node);
        if !arena.is_valid(focus_node) {
            return;
        }
        let Some(caret) = caret_for(arena, focus_node, position) else {
            return;
        };
        if focus_node == active.focus_node && caret == active.focus {
            return;
        }
        if focus_node != active.focus_node {
            tracing::debug!(
                from = %key_name(arena, active.focus_node),
                to = %key_name(arena, focus_node),
                caret,
                "drag focus crossed onto another run"
            );
        }
        let root = self.root;
        let next = ActiveSelection {
            focus_node,
            focus: caret,
            ..active
        };
        let involved = paint_cross_selection(arena, root, &next, &next.involved);
        self.set_focus(arena, Some(focus_node));
        let trace_key = (
            next.anchor_node,
            next.focus_node,
            next.anchor,
            next.focus,
        );
        if self.last_extend_trace != Some(trace_key) {
            self.last_extend_trace = Some(trace_key);
            let range = if next.is_single() {
                expand_for(arena, next.focus_node, next.anchor, next.focus, next.granularity)
                    .map(|s| s.normalized())
            } else {
                None
            };
            tracing::debug!(
                anchor_node = %key_name(arena, next.anchor_node),
                focus_node = %key_name(arena, focus_node),
                anchor = next.anchor,
                focus = caret,
                ?range,
                "drag extended selection"
            );
        }
        self.active_selection = Some(ActiveSelection { involved, ..next });
    }

    /// Finalizes a press on release: applies a deferred inside-collapse.
    pub(crate) fn release_selection(&mut self, arena: &mut TreeArena, click: u32) {
        // A drag-cancelled release carries no click: drop any deferred
        // collapse instead of leaking it into the next gesture (F2).
        if click == 0 {
            self.pending_collapse = None;
            self.pending_press = None;
            return;
        }
        // Resolve a press-before-paint that never dragged (F2): a plain
        // release lands a caret at the press origin.
        if let Some(pending) = self.pending_press.take() {
            if self.active_selection.is_none()
                && arena.is_valid(pending.node)
                && selectable_text(arena, pending.node)
                && !text_is_empty(arena, pending.node)
            {
                let at = self.tracker.press_origin();
                if let Some(caret) = caret_for(arena, pending.node, at) {
                    self.set_focus(arena, Some(pending.node));
                    self.clear_active_selection_except(arena, pending.node);
                    let root = self.root;
                    let active =
                        ActiveSelection::single(pending.node, caret, pending.granularity);
                    let involved = paint_cross_selection(arena, root, &active, &[]);
                    self.active_selection = Some(ActiveSelection { involved, ..active });
                }
            }
        }
        if let Some((node, caret)) = self.pending_collapse.take() {
            if arena.is_valid(node) {
                let granularity = self
                    .active_selection
                    .as_ref()
                    .filter(|a| a.anchor_node == node)
                    .map(|a| a.granularity)
                    .unwrap_or(SelectionGranularity::Character);
                // A plain release collapses to a caret; with a word/paragraph
                // burst the same-node expansion below keeps the double-click
                // word selected instead of collapsing it.
                let root = self.root;
                let prev = self
                    .active_selection
                    .as_ref()
                    .map(|a| a.involved.clone())
                    .unwrap_or_default();
                let active = ActiveSelection::single(node, caret, granularity);
                let involved = paint_cross_selection(arena, root, &active, &prev);
                self.active_selection = Some(ActiveSelection { involved, ..active });
            }
        }
    }

    /// Clears the tracked highlight on every painted run, if any.
    pub(crate) fn clear_active_selection(&mut self, arena: &mut TreeArena) {
        if let Some(active) = self.active_selection.take() {
            tracing::debug!(
                anchor = %key_name(arena, active.anchor_node),
                focus = %key_name(arena, active.focus_node),
                "clearing selection"
            );
            clear_selection(arena, active.anchor_node);
            clear_selection(arena, active.focus_node);
            for id in active.involved {
                clear_selection(arena, id);
            }
        }
    }

    /// Clears the tracked highlight unless it touches `except`.
    ///
    /// Cross-node ranges touching `except` are left painted: callers capture
    /// `involved` beforehand and reconcile leavers through
    /// `paint_cross_selection`, which is the single place that repaints.
    pub(crate) fn clear_active_selection_except(&mut self, arena: &mut TreeArena, except: NodeId) {
        let touches = self.active_selection.as_ref().is_some_and(|a| {
            a.anchor_node == except || a.focus_node == except || a.involved.contains(&except)
        });
        if !touches {
            self.clear_active_selection(arena);
        }
    }

    /// Commits a keyboard-driven range, taking selection ownership from the mouse.
    ///
    /// Keyboard editing is single-node: any cross-node highlight shrinks onto
    /// the edited run first so no stale paint survives on other runs.
    pub(crate) fn apply_key_selection(
        &mut self,
        arena: &mut TreeArena,
        node: NodeId,
        sel: TextSelection,
    ) -> bool {
        self.pending_collapse = None;
        let root = self.root;
        let prev = self
            .active_selection
            .as_ref()
            .map(|a| a.involved.clone())
            .unwrap_or_default();
        let active = ActiveSelection {
            anchor_node: node,
            anchor: sel.anchor,
            focus_node: node,
            focus: sel.focus,
            granularity: SelectionGranularity::Character,
            involved: Vec::new(),
        };
        // Same-node character precision reproduces `sel` exactly; leavers
        // from any previous cross-node range repaint clean via `prev`.
        let involved = paint_cross_selection(arena, root, &active, &prev);
        self.active_selection = Some(ActiveSelection { involved, ..active });
        true
    }

    /// Text under the current highlight, concatenated in document order.
    ///
    /// Single-node ranges yield that run's substring; cross-node ranges join
    /// every painted non-empty sub-range with a newline. Only runs tracked by
    /// the gesture count, so unrelated programmatic highlights never leak in.
    pub(crate) fn clipboard_text_for(&self, arena: &TreeArena) -> Option<String> {
        let Some(active) = self.active_selection.as_ref() else {
            // No gesture tracked (e.g. programmatic highlight): fall back to
            // the focused run's stored range, the previous behavior.
            let node = self.focused?;
            if !arena.is_valid(node) {
                return None;
            }
            let content = match &arena.get(node).kind {
                NodeKind::Text(t) => t.content.clone(),
                _ => return None,
            };
            let sel = arena.get(node).state.text.text_selection?;
            let (s, e) = sel.normalized();
            return (e > s).then(|| content.get(s..e).unwrap_or("").to_string());
        };
        let root = self.root?;
        let mut tracked = active.involved.clone();
        tracked.push(active.anchor_node);
        tracked.push(active.focus_node);
        let mut parts = Vec::new();
        for id in selectable_order(arena, root) {
            if !tracked.contains(&id) || !arena.is_valid(id) {
                continue;
            }
            let content = match &arena.get(id).kind {
                NodeKind::Text(t) => t.content.clone(),
                _ => continue,
            };
            let Some(sel) = arena.get(id).state.text.text_selection else {
                continue;
            };
            let (s, e) = sel.normalized();
            if e > s {
                parts.push(content.get(s..e).unwrap_or("").to_string());
            }
        }
        if parts.is_empty() {
            return None;
        }
        Some(parts.join("\n"))
    }
    /// Moves keyboard focus, repainting the caret on both ends.
    pub(crate) fn set_focus(&mut self, arena: &mut TreeArena, node: Option<NodeId>) {
        let node = node.filter(|n| arena.is_valid(*n));
        if self.focused == node {
            return;
        }
        if let Some(old) = self.focused.take() {
            if arena.is_valid(old) {
                arena.get_mut(old).state.input.text_focused = false;
                arena.mark_dirty(old, DirtyFlags::PAINT);
            }
        }
        self.focused = node;
        if let Some(n) = node {
            arena.get_mut(n).state.input.text_focused = true;
            arena.mark_dirty(n, DirtyFlags::PAINT);
        }
        self.key_goal_x = None;
    }

}

/// Whether a node is opt-in selectable text.
#[inline]
pub(crate) fn selectable_text(arena: &TreeArena, id: NodeId) -> bool {
    matches!(&arena.get(id).kind, NodeKind::Text(t) if t.selectable)
}

/// Display name of a node for diagnostics; never panics on stale ids.
pub(crate) fn key_name(arena: &TreeArena, id: NodeId) -> String {
    if !arena.is_valid(id) {
        return "(invalid)".to_string();
    }
    arena
        .get(id)
        .key
        .clone()
        .map(|k| k.0)
        .unwrap_or_else(|| "(unkeyed)".to_string())
}

/// Whether a text node has no content to select over (F1).
#[inline]
fn text_is_empty(arena: &TreeArena, id: NodeId) -> bool {
    match &arena.get(id).kind {
        NodeKind::Text(t) => t.content.is_empty(),
        _ => true,
    }
}

/// Byte length of a text node's content (0 for anything else).
#[inline]
fn content_len(arena: &TreeArena, id: NodeId) -> usize {
    match &arena.get(id).kind {
        NodeKind::Text(t) => t.content.len(),
        _ => 0,
    }
}

/// Selectable, non-empty text runs in document order (preorder DFS).
///
/// Passthrough nodes contribute their children (they are invisible to
/// collision themselves but their subtree stays hittable); invalid slots are
/// skipped so recycled ids never enter a range.
fn selectable_order(arena: &TreeArena, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !arena.is_valid(id) {
            continue;
        }
        if selectable_text(arena, id) && !text_is_empty(arena, id) {
            out.push(id);
        }
        let children = arena.children(id);
        for &child in children.iter().rev() {
            stack.push(child);
        }
    }
    out
}

/// Desired per-node sub-ranges for a gesture, in document order.
///
/// Same-node gestures expand by the latched granularity (word/paragraph
/// latch survives the drag). Once anchor and focus sit on different runs the
/// edges use character precision, except word presses which snap their own
/// edge to its word; every run strictly between paints fully covered. As with
/// single-node paint, collapsed selections paint nothing here — the caller
/// still stores the caret so keyboard focus and blink keep working.
fn cross_ranges(
    arena: &TreeArena,
    root: Option<NodeId>,
    active: &ActiveSelection,
) -> Vec<(NodeId, TextSelection, SelectionGranularity)> {
    let Some(root) = root else {
        // Without a mount root only the anchor run can resolve; the drag
        // reconciles the rest once a root is known.
        if arena.is_valid(active.anchor_node) {
            let anchor = snap_to_boundary(
                &node_content(arena, active.anchor_node),
                active.anchor.min(content_len(arena, active.anchor_node)),
            );
            let focus = snap_to_boundary(
                &node_content(arena, active.anchor_node),
                active.focus.min(content_len(arena, active.anchor_node)),
            );
            if let Some(sel) = expand_for(arena, active.anchor_node, anchor, focus, active.granularity) {
                return vec![(active.anchor_node, sel, active.granularity)];
            }
        }
        return vec![];
    };
    let order = selectable_order(arena, root);
    let ia = order.iter().position(|id| *id == active.anchor_node);
    let ib = order.iter().position(|id| *id == active.focus_node);
    let (Some(ia), Some(ib)) = (ia, ib) else {
        return vec![];
    };
    if active.anchor_node == active.focus_node {
        let len = content_len(arena, active.anchor_node);
        let anchor = snap_to_boundary(&node_content(arena, active.anchor_node), active.anchor.min(len));
        let focus = snap_to_boundary(&node_content(arena, active.anchor_node), active.focus.min(len));
        if let Some(sel) = expand_for(arena, active.anchor_node, anchor, focus, active.granularity) {
            if sel.is_collapsed() {
                return vec![(active.anchor_node, sel, active.granularity)];
            }
            return vec![(active.anchor_node, sel, active.granularity)];
        }
        return vec![];
    }
    // Cross-node: walk the document span, painting edges and interior.
    let (lo, hi) = if ia <= ib { (ia, ib) } else { (ib, ia) };
    let forward = ia <= ib;
    let mut out = Vec::with_capacity(hi - lo + 1);
    for (i, id) in order.iter().copied().enumerate() {
        if i < lo || i > hi {
            continue;
        }
        let len = content_len(arena, id);
        if id == active.anchor_node {
            let a = snap_to_boundary(&node_content(arena, id), active.anchor.min(len));
            let (s, e) = if forward {
                let s = if active.granularity == SelectionGranularity::Word {
                    word_bounds(&node_content(arena, id), a).0
                } else {
                    a
                };
                (s, len)
            } else {
                let e = if active.granularity == SelectionGranularity::Word {
                    word_bounds(&node_content(arena, id), a).1
                } else {
                    a
                };
                (0, e)
            };
            if e > s {
                // Store the snapped edge (s/e), direction-preserving, so the
                // painted highlight equals the computed range exactly.
                out.push((id, TextSelection { anchor: if forward { s } else { e }, focus: if forward { len } else { 0 } }, SelectionGranularity::Character));
            }
        } else if id == active.focus_node {
            let f = snap_to_boundary(&node_content(arena, id), active.focus.min(len));
            let (s, e) = if forward {
                let e = if active.granularity == SelectionGranularity::Word {
                    word_bounds(&node_content(arena, id), f).1
                } else {
                    f
                };
                (0, e)
            } else {
                let s = if active.granularity == SelectionGranularity::Word {
                    word_bounds(&node_content(arena, id), f).0
                } else {
                    f
                };
                (s, len)
            };
            if e > s {
                out.push((id, TextSelection { anchor: if forward { 0 } else { len }, focus: if forward { e } else { s } }, SelectionGranularity::Character));
            }
        } else if len > 0 {
            let fwd = forward;
            out.push((
                id,
                if fwd {
                    TextSelection { anchor: 0, focus: len }
                } else {
                    TextSelection { anchor: len, focus: 0 }
                },
                SelectionGranularity::Character,
            ));
        }
    }
    out
}

/// Cloned content of a text node (empty for anything else).
fn node_content(arena: &TreeArena, id: NodeId) -> String {
    match &arena.get(id).kind {
        NodeKind::Text(t) => t.content.clone(),
        _ => String::new(),
    }
}

/// Reconciles paint for a gesture: clears runs that left the range, writes
/// every desired sub-range (paint only, never layout), and returns the new
/// `involved` tracking. Collapsed same-node carets still store (so focus and
/// blink work) but highlight nothing, as before.
fn paint_cross_selection(
    arena: &mut TreeArena,
    root: Option<NodeId>,
    active: &ActiveSelection,
    prev_involved: &[NodeId],
) -> Vec<NodeId> {
    let desired = cross_ranges(arena, root, active);
    // Same-node collapsed caret: store it exactly like the old single-node
    // path did, and clear every other run.
    if desired.len() == 1 && desired[0].0 == active.anchor_node && active.is_single() {
        let (id, sel, gran) = desired[0];
        for prev in prev_involved {
            if *prev != id && arena.is_valid(*prev) {
                clear_selection(arena, *prev);
            }
        }
        if sel.is_collapsed() {
            // Keep the caret stored (focus/blink) without a highlight.
            if arena.is_valid(id) {
                arena.get_mut(id).state.text.text_selection = Some(sel);
                arena.get_mut(id).state.text.selection_granularity = gran;
                arena.mark_dirty(id, DirtyFlags::PAINT);
            }
            return vec![id];
        }
        apply_selection(arena, id, sel, gran);
        return vec![id];
    }
    for prev in prev_involved {
        if !desired.iter().any(|(id, _, _)| id == prev) {
            clear_selection(arena, *prev);
        }
    }
    // Runs that were painted but are no longer desired AND are not tracked
    // (e.g. anchor/focus nodes from a previous paint) still clear: sweep any
    // painted run outside the desired set within the old span is handled by
    // clearing anchor/focus nodes when they leave below.
    let mut involved = Vec::with_capacity(desired.len());
    for (id, sel, gran) in desired {
        apply_selection(arena, id, sel, gran);
        involved.push(id);
    }
    // Anchor/focus runs that painted nothing this round (zero-length edge)
    // must not keep a stale highlight from an earlier step.
    for id in [active.anchor_node, active.focus_node] {
        if !involved.contains(&id) {
            clear_selection(arena, id);
        }
    }
    involved
}

/// Current highlight stored on a node, if it is selectable text.
pub(crate) fn current_selection(arena: &TreeArena, node: NodeId) -> Option<TextSelection> {
    if !arena.is_valid(node) {
        return None;
    }
    arena.get(node).state.text.text_selection
}

/// Maps a window point to a caret offset in a text node.
///
/// Translation-only: the node's absolute origin is subtracted, so scaled or
/// rotated text maps approximately and clamps into range. Full unprojection
/// through the accumulated transform is the follow-up.
fn caret_for(arena: &TreeArena, node: NodeId, position: Point) -> Option<usize> {
    let node_ref = arena.get(node);
    let (layout, content) = match &node_ref.kind {
        NodeKind::Text(t) => (
            node_ref.state.text.cached_text_layout.clone()?,
            t.content.as_str(),
        ),
        _ => return None,
    };
    let origin = arena.node_absolute_point(node);
    let local = Point::new(position.x - origin.x, position.y - origin.y);
    Some(caret_at_point(&layout, content, local))
}

/// Expands an anchor/focus pair against a node's content.
fn expand_for(
    arena: &TreeArena,
    node: NodeId,
    anchor: usize,
    focus: usize,
    granularity: SelectionGranularity,
) -> Option<TextSelection> {
    match &arena.get(node).kind {
        NodeKind::Text(t) => Some(expand_selection(&t.content, anchor, focus, granularity)),
        _ => None,
    }
}

/// Writes a node's highlight and repaints it (never re-lays out).
fn apply_selection(
    arena: &mut TreeArena,
    node: NodeId,
    selection: TextSelection,
    granularity: SelectionGranularity,
) {
    if !arena.is_valid(node) {
        return;
    }
    // Never store a highlight on empty content (F1).
    if text_is_empty(arena, node) {
        return;
    }
    arena.get_mut(node).state.text.text_selection = Some(selection);
    arena.get_mut(node).state.text.selection_granularity = granularity;
    arena.mark_dirty(node, DirtyFlags::PAINT);
}

/// Removes a node's highlight and repaints it.
fn clear_selection(arena: &mut TreeArena, node: NodeId) {
    if !arena.is_valid(node) {
        return;
    }
    if arena.get(node).state.text.text_selection.is_some() {
        arena.get_mut(node).state.text.text_selection = None;
        arena.mark_dirty(node, DirtyFlags::PAINT);
    }
}
