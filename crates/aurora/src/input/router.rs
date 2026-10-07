// Single responsibility: Routing pointer events through the hit chain to per-node handlers.

use crate::foundation::Point;
use crate::input::event::{ClickInfo, HoverInfo, PointerEvent, PointerPhase};
use crate::input::handlers::Propagation;
use crate::input::hit::{hit_chain, HitNode, HitTest};
use crate::input::tracker::{PointerTracker, TrackerChange};
use crate::tree::{NodeId, TreeArena};

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

    /// Advances engine time, which pointer velocity is measured against.
    pub fn advance_clock(&mut self, dt: f32) {
        self.clock += dt;
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
    }

    /// Delivers one pointer sample and returns the resulting state.
    pub fn dispatch(
        &mut self,
        arena: &TreeArena,
        root: NodeId,
        event: &PointerEvent,
    ) -> PointerOutcome {
        // Re-resolving the chain per sample is what makes a drag follow the pointer onto
        // whatever is now under it, while the *captured* node still owns the gesture.
        self.chain = hit_chain(arena, root, event.position);
        // Read capture before folding the sample in: a release ends the gesture, so by the
        // time the change is known the capture it must be delivered to is already gone.
        let captured = self.tracker.capture();
        let change = self.tracker.feed(event, self.clock);

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
                    .position(|hit| absorbs(arena, hit.id))
                {
                    let target = self.chain[index].id;
                    self.tracker.set_capture(Some(target));
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
                    let draggable = self
                        .press_chain
                        .iter()
                        .any(|id| arena.get(*id).state.handlers.handles_drag());
                    self.tracker.set_drag_enabled(draggable);
                    if absorbs_handlers_only(arena, target) {
                        for &id in &self.press_chain {
                            let (fired, stopped) =
                                fire(arena, id, |h| h.fire_pointer_down(event));
                            handled |= fired;
                            if stopped {
                                break;
                            }
                        }
                    } else {
                        handled |= absorbed;
                    }
                } else {
                    // A press that bound to nothing has no drag handlers by definition.
                    self.tracker.set_drag_enabled(false);
                }
            }
            TrackerChange::Moved => {
                handled |= self.update_hover(arena, event.position);
                // Steps stream only once the threshold has been crossed. A held button
                // on its own is a press, not a drag: firing steps for sub-threshold
                // moves makes a 1px click wiggle nudge the view (and still deliver the
                // click, since click_armed survives it) — the click that passes off as
                // a drag. Browsers draw the same line with their drag slop.
                if self.tracker.is_dragging() {
                    handled |= self.deliver_to(
                        arena,
                        captured,
                        |h, info| h.fire_drag_step(info),
                        event.position,
                    );
                }
            }
            TrackerChange::DragStarted => {
                handled |= self.deliver_to(
                    arena,
                    captured,
                    |h, info| h.fire_drag_start(info),
                    event.position,
                );
            }
            TrackerChange::Dragged => {
                handled |= self.deliver_to(
                    arena,
                    captured,
                    |h, info| h.fire_drag_step(info),
                    event.position,
                );
            }
            TrackerChange::Released { click } => {
                // The release goes to the views that took the press, not to whatever is
                // under the pointer now, so a click cannot land on a different view.
                // Same target-first (bubbling) order as the press: the press bound
                // target-outward, so the release unwinds the same chain.
                for &id in self.press_chain.iter() {
                    let (fired_up, stopped_up) =
                        fire(arena, id, |h| h.fire_pointer_up(event));
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
                            fire(arena, id, |h| h.fire_click(&info));
                        handled |= fired_click;
                        if stopped_click {
                            break;
                        }
                    }
                }
                self.press_chain.clear();
                handled |= self.update_hover(arena, event.position);
            }
            TrackerChange::DragEnded => {
                let position = event.position;
                let info = self.tracker.drag_info(position, false);
                if let Some(target) = captured {
                    handled |= fire(arena, target, |h| h.fire_drag_end(&info)).0;
                }
                self.press_chain.clear();
                handled |= self.update_hover(arena, position);
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
}

/// Whether a node absorbs a press/hover: it has handlers, or it is Opaque dead
/// space that must not ghost through to nodes beneath.
#[inline]
fn absorbs(arena: &TreeArena, id: NodeId) -> bool {
    !arena.get(id).state.handlers.is_empty()
        || arena.get(id).style.hit_test == HitTest::Opaque
}

/// Whether a node has handlers to fire (vs. absorbing silently as Opaque space).
#[inline]
fn absorbs_handlers_only(arena: &TreeArena, id: NodeId) -> bool {
    !arena.get(id).state.handlers.is_empty()
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
    match arena.get(id).state.handlers.get() {
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
