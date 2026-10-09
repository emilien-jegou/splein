// Single responsibility: Routing pointer events through the hit chain to per-node handlers.

use aurora_foundation::Point;
use aurora_foundation::Key;
use crate::input::event::{ClickInfo, CursorIcon, HoverInfo, PointerEvent, PointerPhase};
use crate::input::handlers::Propagation;
use crate::input::hit::{hit_chain, HitNode, HitTest};
use crate::input::keyboard::{KeyCode, KeyEvent};
use crate::input::tracker::{PointerTracker, TrackerChange};
use crate::text::selection::{
    caret_at_point, caret_line_col, expand_selection, line_byte_ranges, move_grapheme,
    offset_at_visual_line, snap_to_boundary, word_bounds, word_edge, SelectionGranularity,
    TextSelection,
};
use crate::tree::{DirtyFlags, NodeId, NodeKind, TreeArena};

/// Resolved pointer state for a frame, after dispatch.
#[derive(Default, Debug)]
pub struct PointerOutcome {
    /// Key of the node on top at the cursor, when one was hit.
    pub hovered: Option<aurora_foundation::Key>,
    /// Node that received the press, for gesture ownership.
    pub captured: Option<NodeId>,
    /// Whether a gesture is active.
    pub dragging: bool,
    /// Whether any handler wrote to the tree during this dispatch.
    pub handled: bool,
}

/// Routes pointer samples to handlers.
///
/// Dispatch is deliberately separate from hit testing: hit resolution answers "what is on
/// top", and this answers "who is owed the event", which additionally involves press capture,
/// hover transitions and bubbling to ancestors.
pub struct PointerRouter {
    tracker: PointerTracker,
    /// Chain resolved for the current cursor position, reused across the samples in a frame.
    chain: Vec<HitNode>,
    /// Node that most recently received a hover-enter, so leave can be matched to enter.
    hovered: Option<NodeId>,
    /// Nodes that received the current press, innermost first.
    ///
    /// Held from the press rather than recomputed on release, so a release lands on the
    /// views that were actually pressed even if the pointer has since moved off them.
    press_chain: Vec<NodeId>,
    /// Time the engine considers current, advanced once per frame.
    clock: f32,
    /// Wall time the engine considers current, for click-burst expiry.
    wall_now: f32,
    /// Ongoing native text selection (readonly).
    ///
    /// Cross-node (F4): the anchor stays fixed at the press point while the
    /// focus follows the pointer across runs. Every covered run paints its
    /// own sub-range (`involved` tracks them so leavers repaint clean).
    active_selection: Option<ActiveSelection>,
    /// Node owning keyboard focus for selection editing, if any.
    focused: Option<NodeId>,
    /// Target of the previous press, for per-target click bursts.
    ///
    /// Keyed by stable element `Key` (falling back to "no key" for unkeyed
    /// nodes) so remounts that recycle `NodeId` slots don't merge bursts
    /// across different elements.
    last_press: Option<Option<Key>>,
    /// Press that arrived before its text shaped (no cached layout yet).
    ///
    /// Resolved lazily on the next drag step / release from the tracker's
    /// press origin, so the first press in a real window still selects
    /// instead of dying silently until the second press.
    pending_press: Option<PendingPress>,
    /// Goal x preserved across Up/Down moves, cleared by anything else.
    key_goal_x: Option<f32>,
    /// Last text copied with Ctrl+C (internal clipboard; OS sync is a follow-up).
    clipboard: Option<String>,
    /// Deferred collapse when a press lands inside the existing selection.
    ///
    /// Browsers collapse on release rather than on press, so pressing inside a
    /// selection and dragging starts a fresh range instead of losing it first.
    pending_collapse: Option<(NodeId, usize)>,
    /// Last drag state already reported via trace, so per-frame drag steps
    /// stay quiet while edge crossings still show.
    last_extend_trace: Option<(NodeId, NodeId, usize, usize)>,
    /// Mount root for document-order enumeration (cross-node ranges).
    root: Option<NodeId>,
}

/// Native selection gesture state: the fixed press edge (anchor) and the
/// moving edge (focus), each as a node plus a byte offset, with the
/// click-count granularity latched on press.
///
/// Same-node ranges expand by granularity (word/paragraph); once the focus
/// crosses onto another run the edges downgrade to character precision
/// (word presses snap only their own edge to the word), and every run
/// between anchor and focus in document order paints fully covered.
#[derive(Clone, Debug)]
struct ActiveSelection {
    anchor_node: NodeId,
    anchor: usize,
    focus_node: NodeId,
    focus: usize,
    granularity: SelectionGranularity,
    /// Runs currently painted for this gesture, so runs that leave the range
    /// repaint clean.
    involved: Vec<NodeId>,
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
struct PendingPress {
    node: NodeId,
    granularity: SelectionGranularity,
}

impl PointerRouter {
    /// Creates a router with no pointer contact.
    pub fn new() -> Self {
        Self {
            tracker: PointerTracker::new(),
            chain: Vec::new(),
            hovered: None,
            press_chain: Vec::new(),
            clock: 0.0,
            wall_now: 0.0,
            active_selection: None,
            focused: None,
            last_press: None,
            pending_press: None,
            key_goal_x: None,
            clipboard: None,
            pending_collapse: None,
            last_extend_trace: None,
            root: None,
        }
    }

    /// Latest cursor position, absent while the pointer is outside the window.
    #[inline]
    pub fn cursor(&self) -> Option<Point> {
        self.tracker.cursor
    }

    /// Node currently owning the gesture.
    #[inline]
    pub fn captured(&self) -> Option<NodeId> {
        self.tracker.capture()
    }

    /// Whether a gesture is past the drag threshold.
    #[inline]
    pub fn is_dragging(&self) -> bool {
        self.tracker.is_dragging()
    }

    /// Velocity of the most recent pointer history, for gesture hand-off.
    #[inline]
    pub fn velocity(&self) -> Point {
        self.tracker.velocity()
    }

    /// Cursor shape for the current hover (or drag): I-beam over selectable text.
    pub fn cursor_icon(&self, arena: &TreeArena) -> CursorIcon {
        let node = if self.tracker.is_dragging() {
            self.tracker.capture()
        } else {
            self.hovered
        };
        match node {
            Some(id) if arena.is_valid(id) && selectable_text(arena, id) => CursorIcon::Text,
            _ => CursorIcon::Default,
        }
    }

    /// Node owning keyboard focus for selection editing, if any.
    #[inline]
    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }

    /// Last text copied with Ctrl+C, if any.
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    /// Whether a focused caret is currently painted (collapsed focus).
    ///
    /// The engine keeps frames coming while this is true so the caret can
    /// blink; a static highlight needs no such keep-alive.
    pub fn has_visible_caret(&self, arena: &TreeArena) -> bool {
        match self.focused {
            Some(id) if arena.is_valid(id) => {
                arena.get(id).state.text.text_selection.is_some_and(|s| s.is_collapsed())
            }
            _ => false,
        }
    }

    /// Advances engine time, which pointer velocity is measured against.
    pub fn advance_clock(&mut self, dt: f32) {
        self.clock += dt;
    }

    /// Advances wall time, which click-burst expiry is measured against.
    pub fn advance_wall(&mut self, dt: f32) {
        self.wall_now += dt;
    }

    /// Drops hover state, so a pointer that left the window reports a leave.
    pub fn clear_hover(&mut self) {
        self.hovered = None;
    }

    /// Abandons the current gesture and the hover it produced.
    ///
    /// Distinct from [`Self::clear_hover`] because a pointer that disappeared mid-drag has to
    /// give up its capture as well; see [`crate::input::tracker::PointerTracker::cancel_gesture`].
    pub fn cancel_gesture(&mut self) {
        self.clear_hover();
        self.press_chain.clear();
        self.chain.clear();
        self.tracker.cancel_gesture();
        // The gesture ends but the highlight persists, like a browser keeping
        // the selection across focus loss. Tracking is kept too (`involved`
        // still lists every painted run), so a later foreign press clears all
        // of it instead of leaking stale highlights.
        self.pending_collapse = None;
        self.pending_press = None;
    }

    /// Delivers one pointer sample and returns the resulting state.
    ///
    /// Takes the arena mutably: alongside handler dispatch this owns the
    /// native text-selection state, writing the pressed node's highlight and
    /// dirtying paint per sample. Handler-visible behavior is unchanged —
    /// handlers still only observe through signals.
    pub fn dispatch(
        &mut self,
        arena: &mut TreeArena,
        root: NodeId,
        event: &PointerEvent,
    ) -> PointerOutcome {
        // Re-resolving the chain per sample is what makes a drag follow the pointer onto
        // whatever is now under it, while the *captured* node still owns the gesture.
        self.chain = hit_chain(&*arena, root, event.position);
        self.root = Some(root);
        // Read capture before folding the sample in: a release ends the gesture, so by the
        // time the change is known the capture it must be delivered to is already gone.
        let captured = self.tracker.capture();
        let change = self.tracker.feed(event, self.clock, self.wall_now);

        let mut handled = false;
        match change {
            TrackerChange::Pressed => {
                // The press binds to the topmost absorbing node: one with handlers,
                // or an Opaque node whose dead space must not ghost through.
                // A decorative (Auto, handler-less) or Passthrough wrapper can never
                // swallow a click meant for the control inside it.
                if let Some(index) = self
                    .chain
                    .iter()
                    .position(|hit| absorbs(&*arena, hit.id))
                {
                    let target = self.chain[index].id;
                    self.tracker.set_capture(Some(target));
                    // Multi-click counts are per target: a press on a different
                    // node starts a fresh burst, so opening an accordion item
                    // and promptly clicking its copy still collapses a caret
                    // instead of selecting a word or a paragraph. Keyed by
                    // stable element key so slot recycling can't merge bursts.
                    let target_key = arena.get(target).key.clone();
                    if self.last_press != Some(target_key.clone()) {
                        self.tracker.reset_burst();
                    }
                    self.last_press = Some(target_key);
                    // The chain runs topmost-first, so the target's ancestors are everything
                    // after it. Firing from the target outward lets a chip inside a card be
                    // observed by either. Truncated at the first Opaque node outward: an
                    // opaque menu or modal absorbs the press, so nodes beneath never fire.
                    // An Opaque target without handlers absorbs without firing at all.
                    let mut press_chain: Vec<NodeId> = Vec::new();
                    let mut absorbed = false;
                    for hit in &self.chain[index..] {
                        press_chain.push(hit.id);
                        if arena.get(hit.id).style.hit_test == HitTest::Opaque {
                            absorbed = true;
                            break;
                        }
                    }
                    self.press_chain = press_chain;
                    // A drag only exists for content that can be dragged. If nobody
                    // in the pressed chain handles drags, this press stays a press no
                    // matter how far it moves, and release still clicks — the web's
                    // `dragstart`-only-if-draggable rule, and what keeps an ordinary
                    // click that drifts past the threshold from dying silently.
                    // Selectable text is natively draggable (selection drag needs
                    // no app-declared handlers).
                    let draggable = self.press_chain.iter().any(|id| {
                        arena.get(*id).state.input.handlers.handles_drag()
                            || selectable_text(&*arena, *id)
                    });
                    self.tracker.set_drag_enabled(draggable);
                    tracing::debug!(
                        position = ?event.position,
                        target = ?arena.get(target).key,
                        burst = self.tracker.burst_count(),
                        draggable,
                        "press bound",
                    );
                    if absorbs_handlers_only(&*arena, target) {
                        for &id in &self.press_chain {
                            let (fired, stopped) =
                                fire(&*arena, id, |h| h.fire_pointer_down(event));
                            handled |= fired;
                            if stopped {
                                break;
                            }
                        }
                    } else {
                        handled |= absorbed;
                    }
                    self.press_selection(arena, event);
                } else {
                    // A press into the void still arms a drag so sweeping into
                    // selectable text mid-gesture can latch (F3). There are no
                    // handlers to starve here, and release still clears below.
                    tracing::debug!(
                        position = ?event.position,
                        chain_len = self.chain.len(),
                        "press hit nothing absorbing",
                    );
                    self.tracker.set_drag_enabled(true);
                    // Clicking empty space clears the highlight, like a browser.
                    self.clear_active_selection(arena);
                    self.pending_collapse = None;
                    self.pending_press = None;
                    self.set_focus(arena, None);
                    // A press into the void breaks the burst like any retarget.
                    self.tracker.reset_burst();
                    self.last_press = None;
                }
            }
            TrackerChange::Moved => {
                handled |= self.update_hover(&*arena, event.position);
                // Steps stream only once the threshold has been crossed. A held button
                // on its own is a press, not a drag: firing steps for sub-threshold
                // moves makes a 1px click wiggle nudge the view (and still deliver the
                // click, since click_armed survives it) — the click that passes off as
                // a drag. Browsers draw the same line with their drag slop.
                if self.tracker.is_dragging() {
                    handled |= self.deliver_to(
                        &*arena,
                        captured,
                        |h, info| h.fire_drag_step(info),
                        event.position,
                    );
                    self.extend_active_selection(arena, event.position);
                }
            }
            TrackerChange::DragStarted => {
                handled |= self.deliver_to(
                    &*arena,
                    captured,
                    |h, info| h.fire_drag_start(info),
                    event.position,
                );
                // A real drag cancels the deferred collapse: pressing inside a
                // selection and moving starts a fresh range from the press point.
                self.pending_collapse = None;
                self.extend_active_selection(arena, event.position);
            }
            TrackerChange::Dragged => {
                handled |= self.deliver_to(
                    &*arena,
                    captured,
                    |h, info| h.fire_drag_step(info),
                    event.position,
                );
                self.extend_active_selection(arena, event.position);
            }
            TrackerChange::Released { click } => {
                // The release goes to the views that took the press, not to whatever is
                // under the pointer now, so a click cannot land on a different view.
                // Same target-first (bubbling) order as the press: the press bound
                // target-outward, so the release unwinds the same chain.
                for &id in self.press_chain.iter() {
                    let (fired_up, stopped_up) =
                        fire(&*arena, id, |h| h.fire_pointer_up(event));
                    handled |= fired_up;
                    if stopped_up {
                        break;
                    }
                    if click > 0 {
                        let info = ClickInfo {
                            position: event.position,
                            click_count: click,
                        };
                        let (fired_click, stopped_click) =
                            fire(&*arena, id, |h| h.fire_click(&info));
                        handled |= fired_click;
                        if stopped_click {
                            break;
                        }
                    }
                }
                self.press_chain.clear();
                self.release_selection(arena, click);
                handled |= self.update_hover(&*arena, event.position);
            }
            TrackerChange::DragEnded => {
                let position = event.position;
                let info = self.tracker.drag_info(position, false);
                if let Some(target) = captured {
                    handled |= fire(&*arena, target, |h| h.fire_drag_end(&info)).0;
                }
                self.press_chain.clear();
                // The highlight stays selected and stays tracked: a later
                // foreign press must still find and clear it, and shift-click
                // extends from the same anchor. Only the gesture ends.
                if let Some(active) = &self.active_selection {
                    tracing::debug!(
                        anchor_node = %key_name(arena, active.anchor_node),
                        focus_node = %key_name(arena, active.focus_node),
                        anchor = active.anchor,
                        focus = active.focus,
                        "drag ended; highlight kept"
                    );
                } else {
                    tracing::debug!("drag ended with no selection tracked");
                }
                self.pending_collapse = None;
                handled |= self.update_hover(&*arena, position);
            }
        }

        PointerOutcome {
            hovered: self
                .chain
                .first()
                .and_then(|hit| arena.get(hit.id).key.clone()),
            captured: self.tracker.capture(),
            dragging: self.tracker.is_dragging(),
            handled,
        }
    }

    /// Hands a gesture payload to the capturing node, if it still exists.
    fn deliver_to(
        &mut self,
        arena: &TreeArena,
        target: Option<NodeId>,
        invoke: fn(&crate::input::handlers::HandlerSet, &crate::input::event::DragInfo) -> Propagation,
        position: Point,
    ) -> bool {
        let Some(target) = target else {
            return false;
        };
        if !arena.is_valid(target) {
            // The captured node was unmounted mid-gesture; drop the gesture rather than
            // delivering to whatever recycled its slot.
            self.tracker.set_capture(None);
            return false;
        }
        let info = self.tracker.drag_info(position, true);
        fire(arena, target, |h| invoke(h, &info)).0
    }

    /// Emits enter/leave as the topmost interested node under the cursor changes.
    fn update_hover(&mut self, arena: &TreeArena, position: Point) -> bool {
        let next = self
            .chain
            .iter()
            .find(|hit| absorbs(arena, hit.id))
            .map(|hit| hit.id);
        if next == self.hovered {
            return false;
        }
        let mut handled = false;
        if let Some(leaving) = self.hovered {
            if arena.is_valid(leaving) {
                let info = HoverInfo { position };
                handled |= fire(arena, leaving, |h| h.fire_pointer_leave(&info)).0;
            }
        }
        if let Some(entering) = next {
            let info = HoverInfo { position };
            handled |= fire(arena, entering, |h| h.fire_pointer_enter(&info)).0;
        }
        self.hovered = next;
        handled
    }

    /// Handles the selection half of a press on (or off) selectable text.
    fn press_selection(&mut self, arena: &mut TreeArena, event: &PointerEvent) {
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
    fn extend_active_selection(&mut self, arena: &mut TreeArena, position: Point) {
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
    fn release_selection(&mut self, arena: &mut TreeArena, click: u32) {
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
    fn clear_active_selection(&mut self, arena: &mut TreeArena) {
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
    fn clear_active_selection_except(&mut self, arena: &mut TreeArena, except: NodeId) {
        let touches = self.active_selection.as_ref().is_some_and(|a| {
            a.anchor_node == except || a.focus_node == except || a.involved.contains(&except)
        });
        if !touches {
            self.clear_active_selection(arena);
        }
    }

    /// Moves keyboard focus, repainting the caret on both ends.
    fn set_focus(&mut self, arena: &mut TreeArena, node: Option<NodeId>) {
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

    /// Applies one key press to the focused selectable text.
    ///
    /// Returns whether anything changed (and so whether the caller owes a
    /// redraw). Readonly semantics: movement, select-all, copy, and collapse;
    /// typing and deletion are no-ops.
    pub fn dispatch_key(&mut self, arena: &mut TreeArena, key: &KeyEvent) -> bool {
        let Some(node) = self
            .focused
            .filter(|n| arena.is_valid(*n) && selectable_text(arena, *n))
        else {
            return false;
        };
        let (content, layout) = match &arena.get(node).kind {
            NodeKind::Text(t) => (
                t.content.clone(),
                arena.get(node).state.text.cached_text_layout.clone(),
            ),
            _ => return false,
        };
        let Some(layout) = layout else { return false };
        if content.is_empty() || layout.lines.is_empty() {
            return false;
        }
        let len = content.len();
        let clamp = |o: usize| snap_to_boundary(&content, o.min(len));
        let current = current_selection(arena, node);
        let caret = current.map(|s| s.focus).unwrap_or(0).min(len);
        let anchor = current.map(|s| s.anchor).unwrap_or(caret).min(len);
        let collapsed = current.is_none_or(|s| s.is_collapsed());

        // Copy reads the range but repaints nothing.
        if matches!(key.code, KeyCode::Char(_)) && key.command_modifier() {
            if let KeyCode::Char(c) = key.code {
                match c.to_ascii_lowercase() {
                    'a' => {
                        return self.apply_key_selection(
                            arena,
                            node,
                            TextSelection { anchor: 0, focus: len },
                        );
                    }
                    'c' => {
                        if let Some(text) = self.clipboard_text_for(arena) {
                            if !text.is_empty() {
                                self.clipboard = Some(text);
                            }
                        }
                        return false;
                    }
                    _ => return false,
                }
            }
        }

        let shift = key.shift;
        let extend = |a: usize, f: usize| {
            if shift {
                (a, f)
            } else {
                (f, f)
            }
        };
        let mut keep_goal = false;
        let next: Option<(usize, usize)> = match key.code {
            KeyCode::Escape => Some((caret, caret)),
            KeyCode::Left => {
                if key.word_modifier() {
                    let f = word_edge(&content, caret, -1);
                    Some(extend(anchor, f))
                } else if !shift && !collapsed {
                    let c = anchor.min(caret);
                    Some((c, c))
                } else {
                    let f = move_grapheme(&content, caret, -1);
                    Some(extend(anchor, f))
                }
            }
            KeyCode::Right => {
                if key.word_modifier() {
                    let f = word_edge(&content, caret, 1);
                    Some(extend(anchor, f))
                } else if !shift && !collapsed {
                    let c = anchor.max(caret);
                    Some((c, c))
                } else {
                    let f = move_grapheme(&content, caret, 1);
                    Some(extend(anchor, f))
                }
            }
            KeyCode::Up | KeyCode::Down => {
                let dir = if matches!(key.code, KeyCode::Up) {
                    -1
                } else {
                    1
                };
                let (line, x) = match caret_line_col(&layout, &content, caret) {
                    Some(v) => v,
                    None => return false,
                };
                let gx = self.key_goal_x.unwrap_or(x);
                let target = (line as i32 + dir)
                    .clamp(0, layout.lines.len() as i32 - 1)
                    as usize;
                let Some(f) = offset_at_visual_line(&layout, &content, target, gx)
                else {
                    return false;
                };
                self.key_goal_x = Some(gx);
                keep_goal = true;
                Some(extend(anchor, f))
            }
            KeyCode::Home | KeyCode::End => {
                let f = if key.word_modifier() {
                    if matches!(key.code, KeyCode::Home) {
                        0
                    } else {
                        len
                    }
                } else {
                    let Some((line, _)) = caret_line_col(&layout, &content, caret)
                    else {
                        return false;
                    };
                    let ranges = line_byte_ranges(&layout, &content);
                    let (ls, le) = ranges[line];
                    if matches!(key.code, KeyCode::Home) {
                        ls
                    } else {
                        le
                    }
                };
                Some(extend(anchor, f))
            }
            KeyCode::Char(_) => return false,
        };
        if !keep_goal {
            self.key_goal_x = None;
        }
        let Some((a, f)) = next else { return false };
        let sel = TextSelection {
            anchor: clamp(a),
            focus: clamp(f),
        };
        if current == Some(sel) {
            return false;
        }
        self.apply_key_selection(arena, node, sel)
    }

    /// Commits a keyboard-driven range, taking selection ownership from the mouse.
    ///
    /// Keyboard editing is single-node: any cross-node highlight shrinks onto
    /// the edited run first so no stale paint survives on other runs.
    fn apply_key_selection(
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
    fn clipboard_text_for(&self, arena: &TreeArena) -> Option<String> {
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
}

/// Whether a node is opt-in selectable text.
#[inline]
fn selectable_text(arena: &TreeArena, id: NodeId) -> bool {
    matches!(&arena.get(id).kind, NodeKind::Text(t) if t.selectable)
}

/// Display name of a node for diagnostics; never panics on stale ids.
fn key_name(arena: &TreeArena, id: NodeId) -> String {
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
fn current_selection(arena: &TreeArena, node: NodeId) -> Option<TextSelection> {
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

/// Whether a node absorbs a press/hover: it has handlers, selectable text, or
/// it is Opaque dead space that must not ghost through to nodes beneath.
#[inline]
fn absorbs(arena: &TreeArena, id: NodeId) -> bool {
    !arena.get(id).state.input.handlers.is_empty()
        || arena.get(id).style.hit_test == HitTest::Opaque
        || selectable_text(arena, id)
}

/// Whether a node has handlers to fire (vs. absorbing silently as Opaque space).
#[inline]
fn absorbs_handlers_only(arena: &TreeArena, id: NodeId) -> bool {
    !arena.get(id).state.input.handlers.is_empty()
}

/// Invokes one handler slot on a node, if it has one.
///
/// Returns `(fired, stopped)`: whether a handler ran, and whether it returned
/// [`Propagation::Stop`] to halt bubbling to ancestors.
#[inline]
fn fire(
    arena: &TreeArena,
    id: NodeId,
    invoke: impl FnOnce(&crate::input::handlers::HandlerSet) -> Propagation,
) -> (bool, bool) {
    if !arena.is_valid(id) {
        return (false, false);
    }
    match arena.get(id).state.input.handlers.get() {
        Some(handlers) => {
            let propagation = invoke(handlers);
            (true, propagation == Propagation::Stop)
        }
        None => (false, false),
    }
}

impl Default for PointerRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience for callers that only need the phase of an event.
#[inline]
pub fn is_press(event: &PointerEvent) -> bool {
    event.phase == PointerPhase::Down
}
