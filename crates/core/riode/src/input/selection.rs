// Single responsibility: Readonly text selection gestures: press, drag-extend, release,
// keyboard selection, and clipboard reads.

use super::router::PointerRouter;
use crate::foundation::Point;
use crate::input::event::PointerEvent;
use crate::text::selection::{
    caret_at_point, expand_selection, snap_to_boundary, word_bounds, SelectionGranularity,
    TextSelection,
};
use crate::tree::TreeArena;
use crate::tree::{DirtyFlags, NodeId};

#[derive(Clone, Debug)]
pub(crate) struct ActiveSelection {
    pub(crate) anchor_node: NodeId,
    pub(crate) anchor: usize,
    pub(crate) focus_node: NodeId,
    pub(crate) focus: usize,
    pub(crate) granularity: SelectionGranularity,
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

#[derive(Copy, Clone, Debug)]
pub(crate) struct PendingPress {
    node: NodeId,
    granularity: SelectionGranularity,
}

impl PointerRouter {
    pub(crate) fn press_selection(&mut self, arena: &mut TreeArena, event: &PointerEvent) {
        self.last_extend_trace = None;
        let target = self
            .press_chain
            .first()
            .copied()
            .filter(|id| selectable_text(arena, *id));
        let Some(node) = target else {
            tracing::debug!(position = ?event.position, "press hit no selectable text; clearing selection");
            self.clear_active_selection(arena);
            self.pending_collapse = None;
            self.pending_press = None;
            let viewport = self.press_chain.iter().copied().find(|id| {
                let style = &arena.get(*id).style;
                style.scrollable && style.clip
            });
            self.set_focus(arena, viewport);
            return;
        };

        if text_is_empty(arena, node) {
            self.clear_active_selection(arena);
            self.pending_collapse = None;
            self.pending_press = None;
            self.set_focus(arena, None);
            return;
        }

        let granularity = match self.tracker.burst_count() {
            0 => SelectionGranularity::Character,
            1 => SelectionGranularity::Word,
            _ => SelectionGranularity::Paragraph,
        };
        let Some(caret) = caret_for(arena, node, event.position) else {
            tracing::debug!(node = ?arena.get(node).key, "press with no shaped layout; deferring anchor");
            self.set_focus(arena, Some(node));
            self.clear_active_selection_except(arena, node);
            self.pending_collapse = None;
            self.pending_press = Some(PendingPress { node, granularity });
            self.active_selection = None;
            return;
        };
        self.pending_press = None;
        self.set_focus(arena, Some(node));

        if event.shift {
            let anchored = self
                .active_selection
                .as_ref()
                .filter(|a| arena.is_valid(a.anchor_node))
                .map(|a| (a.anchor_node, a.anchor, a.granularity))
                .or_else(|| {
                    self.focused.filter(|f| *f != node).and_then(|f| {
                        current_selection(arena, f).map(|s| (f, s.anchor, granularity))
                    })
                })
                .or_else(|| current_selection(arena, node).map(|s| (node, s.anchor, granularity)));
            if let Some((anchor_node, anchor, gran)) = anchored {
                let root = self.root;
                let prev = self.active_selection.as_ref().map(|a| a.involved.clone()).unwrap_or_default();
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

        let prev = self.active_selection.as_ref().map(|a| a.involved.clone()).unwrap_or_default();
        self.clear_active_selection_except(arena, node);

        if granularity == SelectionGranularity::Character {
            if let Some(current) = current_selection(arena, node) {
                let (s, e) = current.normalized();
                if !current.is_collapsed() && caret > s && caret < e {
                    self.pending_collapse = Some((node, caret));
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
        self.pending_collapse = None;
        self.active_selection = Some(ActiveSelection { involved, ..active });
    }

    pub(crate) fn extend_active_selection(&mut self, arena: &mut TreeArena, position: Point) {
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
                            let involved = paint_cross_selection(arena, root, &active, &[]);
                            self.active_selection = Some(ActiveSelection { involved, ..active });
                            return;
                        }
                    }
                }
            }
        }

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

        let Some(active) = self.active_selection.clone() else { return; };
        if !arena.is_valid(active.anchor_node) {
            self.active_selection = None;
            return;
        }

        if self.tracker.capture().is_some_and(|c| c != active.anchor_node) {
            return;
        }

        let focus_node = self
            .chain
            .iter()
            .map(|h| h.id)
            .filter(|id| arena.is_valid(*id) && selectable_text(arena, *id))
            .find(|id| !text_is_empty(arena, *id))
            .unwrap_or(active.focus_node);
        if !arena.is_valid(focus_node) { return; }

        let Some(caret) = caret_for(arena, focus_node, position) else { return; };
        if focus_node == active.focus_node && caret == active.focus { return; }

        let root = self.root;
        let next = ActiveSelection { focus_node, focus: caret, ..active };
        let involved = paint_cross_selection(arena, root, &next, &next.involved);
        self.set_focus(arena, Some(focus_node));
        self.active_selection = Some(ActiveSelection { involved, ..next });
    }

    pub(crate) fn release_selection(&mut self, arena: &mut TreeArena, click: u32) {
        if click == 0 {
            self.pending_collapse = None;
            self.pending_press = None;
            return;
        }
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
                    let active = ActiveSelection::single(pending.node, caret, pending.granularity);
                    let involved = paint_cross_selection(arena, root, &active, &[]);
                    self.active_selection = Some(ActiveSelection { involved, ..active });
                }
            }
        }
        if let Some((node, caret)) = self.pending_collapse.take() {
            if arena.is_valid(node) {
                let granularity = self.active_selection.as_ref()
                    .filter(|a| a.anchor_node == node)
                    .map(|a| a.granularity)
                    .unwrap_or(SelectionGranularity::Character);
                let root = self.root;
                let prev = self.active_selection.as_ref().map(|a| a.involved.clone()).unwrap_or_default();
                let active = ActiveSelection::single(node, caret, granularity);
                let involved = paint_cross_selection(arena, root, &active, &prev);
                self.active_selection = Some(ActiveSelection { involved, ..active });
            }
        }
    }

    pub(crate) fn clear_active_selection(&mut self, arena: &mut TreeArena) {
        if let Some(active) = self.active_selection.take() {
            clear_selection(arena, active.anchor_node);
            clear_selection(arena, active.focus_node);
            for id in active.involved {
                clear_selection(arena, id);
            }
        }
    }

    pub(crate) fn clear_active_selection_except(&mut self, arena: &mut TreeArena, except: NodeId) {
        let touches = self.active_selection.as_ref().is_some_and(|a| {
            a.anchor_node == except || a.focus_node == except || a.involved.contains(&except)
        });
        if !touches {
            self.clear_active_selection(arena);
        }
    }

    pub(crate) fn apply_key_selection(&mut self, arena: &mut TreeArena, node: NodeId, sel: TextSelection) -> bool {
        self.pending_collapse = None;
        let root = self.root;
        let prev = self.active_selection.as_ref().map(|a| a.involved.clone()).unwrap_or_default();
        let active = ActiveSelection {
            anchor_node: node,
            anchor: sel.anchor,
            focus_node: node,
            focus: sel.focus,
            granularity: SelectionGranularity::Character,
            involved: Vec::new(),
        };
        let involved = paint_cross_selection(arena, root, &active, &prev);
        self.active_selection = Some(ActiveSelection { involved, ..active });
        true
    }

    pub(crate) fn clipboard_text_for(&self, arena: &TreeArena) -> Option<String> {
        let Some(active) = self.active_selection.as_ref() else {
            let node = self.focused?;
            if !arena.is_valid(node) { return None; }
            let content = arena.get(node).text_content()?;
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
            if !tracked.contains(&id) || !arena.is_valid(id) { continue; }
            let Some(content) = arena.get(id).text_content() else { continue; };
            let Some(sel) = arena.get(id).state.text.text_selection else { continue; };
            let (s, e) = sel.normalized();
            if e > s {
                parts.push(content.get(s..e).unwrap_or("").to_string());
            }
        }
        if parts.is_empty() { return None; }
        Some(parts.join("\n"))
    }

    pub(crate) fn set_focus(&mut self, arena: &mut TreeArena, node: Option<NodeId>) {
        let node = node.filter(|n| arena.is_valid(*n));
        if self.focused == node { return; }
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

#[inline(always)]
pub(crate) fn selectable_text(arena: &TreeArena, id: NodeId) -> bool {
    arena.is_valid(id) && arena.get(id).is_selectable_text()
}

pub(crate) fn key_name(arena: &TreeArena, id: NodeId) -> String {
    if !arena.is_valid(id) { return "(invalid)".to_string(); }
    arena.get(id).key.clone().map(|k| k.0).unwrap_or_else(|| "(unkeyed)".to_string())
}

#[inline(always)]
fn text_is_empty(arena: &TreeArena, id: NodeId) -> bool {
    arena.get(id).is_text_empty()
}

#[inline(always)]
fn content_len(arena: &TreeArena, id: NodeId) -> usize {
    arena.get(id).text_len()
}

fn selectable_order(arena: &TreeArena, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !arena.is_valid(id) { continue; }
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

fn cross_ranges(
    arena: &TreeArena,
    root: Option<NodeId>,
    active: &ActiveSelection,
) -> Vec<(NodeId, TextSelection, SelectionGranularity)> {
    let Some(root) = root else {
        if arena.is_valid(active.anchor_node) {
            let anchor = snap_to_boundary(&node_content(arena, active.anchor_node), active.anchor.min(content_len(arena, active.anchor_node)));
            let focus = snap_to_boundary(&node_content(arena, active.anchor_node), active.focus.min(content_len(arena, active.anchor_node)));
            if let Some(sel) = expand_for(arena, active.anchor_node, anchor, focus, active.granularity) {
                return vec![(active.anchor_node, sel, active.granularity)];
            }
        }
        return vec![];
    };
    let order = selectable_order(arena, root);
    let ia = order.iter().position(|id| *id == active.anchor_node);
    let ib = order.iter().position(|id| *id == active.focus_node);
    let (Some(ia), Some(ib)) = (ia, ib) else { return vec![]; };
    if active.anchor_node == active.focus_node {
        let len = content_len(arena, active.anchor_node);
        let anchor = snap_to_boundary(&node_content(arena, active.anchor_node), active.anchor.min(len));
        let focus = snap_to_boundary(&node_content(arena, active.anchor_node), active.focus.min(len));
        if let Some(sel) = expand_for(arena, active.anchor_node, anchor, focus, active.granularity) {
            return vec![(active.anchor_node, sel, active.granularity)];
        }
        return vec![];
    }
    let (lo, hi) = if ia <= ib { (ia, ib) } else { (ib, ia) };
    let forward = ia <= ib;
    let mut out = Vec::with_capacity(hi - lo + 1);
    for (i, id) in order.iter().copied().enumerate() {
        if i < lo || i > hi { continue; }
        let len = content_len(arena, id);
        if id == active.anchor_node {
            let a = snap_to_boundary(&node_content(arena, id), active.anchor.min(len));
            let (s, e) = if forward {
                let s = if active.granularity == SelectionGranularity::Word { word_bounds(&node_content(arena, id), a).0 } else { a };
                (s, len)
            } else {
                let e = if active.granularity == SelectionGranularity::Word { word_bounds(&node_content(arena, id), a).1 } else { a };
                (0, e)
            };
            if e > s {
                out.push((id, TextSelection { anchor: if forward { s } else { e }, focus: if forward { len } else { 0 } }, SelectionGranularity::Character));
            }
        } else if id == active.focus_node {
            let f = snap_to_boundary(&node_content(arena, id), active.focus.min(len));
            let (s, e) = if forward {
                let e = if active.granularity == SelectionGranularity::Word { word_bounds(&node_content(arena, id), f).1 } else { f };
                (0, e)
            } else {
                let s = if active.granularity == SelectionGranularity::Word { word_bounds(&node_content(arena, id), f).0 } else { f };
                (s, len)
            };
            if e > s {
                out.push((id, TextSelection { anchor: if forward { 0 } else { len }, focus: if forward { e } else { s } }, SelectionGranularity::Character));
            }
        } else if len > 0 {
            let fwd = forward;
            out.push((id, if fwd { TextSelection { anchor: 0, focus: len } } else { TextSelection { anchor: len, focus: 0 } }, SelectionGranularity::Character));
        }
    }
    out
}

#[inline(always)]
fn node_content(arena: &TreeArena, id: NodeId) -> String {
    arena.get(id).text_content().unwrap_or("").to_string()
}

fn paint_cross_selection(
    arena: &mut TreeArena,
    root: Option<NodeId>,
    active: &ActiveSelection,
    prev_involved: &[NodeId],
) -> Vec<NodeId> {
    let desired = cross_ranges(arena, root, active);
    if desired.len() == 1 && desired[0].0 == active.anchor_node && active.is_single() {
        let (id, sel, gran) = desired[0];
        for prev in prev_involved {
            if *prev != id && arena.is_valid(*prev) { clear_selection(arena, *prev); }
        }
        if sel.is_collapsed() {
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
        if !desired.iter().any(|(id, _, _)| id == prev) { clear_selection(arena, *prev); }
    }
    let mut involved = Vec::with_capacity(desired.len());
    for (id, sel, gran) in desired {
        apply_selection(arena, id, sel, gran);
        involved.push(id);
    }
    for id in [active.anchor_node, active.focus_node] {
        if !involved.contains(&id) { clear_selection(arena, id); }
    }
    involved
}

pub(crate) fn current_selection(arena: &TreeArena, node: NodeId) -> Option<TextSelection> {
    if !arena.is_valid(node) { return None; }
    arena.get(node).state.text.text_selection
}

fn caret_for(arena: &TreeArena, node: NodeId, position: Point) -> Option<usize> {
    let node_ref = arena.get(node);
    let content = node_ref.text_content()?;
    let layout = node_ref.state.text.cached_text_layout.clone()?;
    let origin = arena.node_absolute_point(node);
    let local = Point::new(position.x - origin.x, position.y - origin.y);
    Some(caret_at_point(&layout, content, local))
}

fn expand_for(
    arena: &TreeArena,
    node: NodeId,
    anchor: usize,
    focus: usize,
    granularity: SelectionGranularity,
) -> Option<TextSelection> {
    let content = arena.get(node).text_content()?;
    Some(expand_selection(content, anchor, focus, granularity))
}

fn apply_selection(arena: &mut TreeArena, node: NodeId, selection: TextSelection, granularity: SelectionGranularity) {
    if !arena.is_valid(node) || text_is_empty(arena, node) { return; }
    arena.get_mut(node).state.text.text_selection = Some(selection);
    arena.get_mut(node).state.text.selection_granularity = granularity;
    arena.mark_dirty(node, DirtyFlags::PAINT);
}

fn clear_selection(arena: &mut TreeArena, node: NodeId) {
    if !arena.is_valid(node) { return; }
    if arena.get(node).state.text.text_selection.is_some() {
        arena.get_mut(node).state.text.text_selection = None;
        arena.mark_dirty(node, DirtyFlags::PAINT);
    }
}
