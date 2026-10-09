// Single responsibility: Routing pointer events through the hit chain to per-node handlers.

use crate::foundation::Point;
use crate::foundation::Key;
use crate::input::event::{ClickInfo, CursorIcon, HoverInfo, PointerEvent, PointerPhase};
use crate::input::handlers::Propagation;
use crate::input::hit::{hit_chain, HitNode, HitTest};
use crate::input::keyboard::{KeyCode, KeyEvent};
use crate::input::tracker::{PointerTracker, TrackerChange};
use super::selection::{ActiveSelection, PendingPress, current_selection, key_name, selectable_text};
use crate::text::selection::{
    caret_line_col, line_byte_ranges, move_grapheme, offset_at_visual_line, snap_to_boundary,
    word_edge, TextSelection,
};
use crate::tree::{NodeId, NodeKind};
use crate::tree::{TreeArena};

/// Resolved pointer state for a frame, after dispatch.
#[derive(Default, Debug)]
pub struct PointerOutcome {
    /// Key of the node on top at the cursor, when one was hit.
    pub hovered: Option<crate::foundation::Key>,
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
    pub(crate) tracker: PointerTracker,
    /// Chain resolved for the current cursor position, reused across the samples in a frame.
    pub(crate) chain: Vec<HitNode>,
    /// Node that most recently received a hover-enter, so leave can be matched to enter.
    pub(crate) hovered: Option<NodeId>,
    /// Nodes that received the current press, innermost first.
    ///
    /// Held from the press rather than recomputed on release, so a release lands on the
    /// views that were actually pressed even if the pointer has since moved off them.
    pub(crate) press_chain: Vec<NodeId>,
    /// Time the engine considers current, advanced once per frame.
    pub(crate) clock: f32,
    /// Wall time the engine considers current, for click-burst expiry.
    pub(crate) wall_now: f32,
    /// Ongoing native text selection (readonly).
    ///
    /// Cross-node (F4): the anchor stays fixed at the press point while the
    /// focus follows the pointer across runs. Every covered run paints its
    /// own sub-range (`involved` tracks them so leavers repaint clean).
    pub(crate) active_selection: Option<ActiveSelection>,
    /// Node owning keyboard focus for selection editing, if any.
    pub(crate) focused: Option<NodeId>,
    /// Target of the previous press, for per-target click bursts.
    ///
    /// Keyed by stable element `Key` (falling back to "no key" for unkeyed
    /// nodes) so remounts that recycle `NodeId` slots don't merge bursts
    /// across different elements.
    pub(crate) last_press: Option<Option<Key>>,
    /// Press that arrived before its text shaped (no cached layout yet).
    ///
    /// Resolved lazily on the next drag step / release from the tracker's
    /// press origin, so the first press in a real window still selects
    /// instead of dying silently until the second press.
    pub(crate) pending_press: Option<PendingPress>,
    /// Goal x preserved across Up/Down moves, cleared by anything else.
    pub(crate) key_goal_x: Option<f32>,
    /// Last text copied with Ctrl+C (internal clipboard; OS sync is a follow-up).
    pub(crate) clipboard: Option<String>,
    /// Deferred collapse when a press lands inside the existing selection.
    ///
    /// Browsers collapse on release rather than on press, so pressing inside a
    /// selection and dragging starts a fresh range instead of losing it first.
    pub(crate) pending_collapse: Option<(NodeId, usize)>,
    /// Last drag state already reported via trace, so per-frame drag steps
    /// stay quiet while edge crossings still show.
    pub(crate) last_extend_trace: Option<(NodeId, NodeId, usize, usize)>,
    /// Mount root for document-order enumeration (cross-node ranges).
    pub(crate) root: Option<NodeId>,
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
                    // matter how far it moves, and release still clicks as long as it
                    // lands back on the press chain — the web's
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
                //
                // Clicks additionally need the pointer: only press-chain nodes still
                // under the pointer click (the web's common-ancestor rule). A press
                // that drifts but releases on its target still clicks; a drag
                // released elsewhere must not toggle what it started on.
                // `self.chain` was just re-resolved at the release position above.
                for &id in self.press_chain.iter() {
                    let (fired_up, stopped_up) =
                        fire(&*arena, id, |h| h.fire_pointer_up(event));
                    handled |= fired_up;
                    if stopped_up {
                        break;
                    }
                    if click > 0 && self.chain.iter().any(|hit| hit.id == id) {
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
