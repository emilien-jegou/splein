// Single responsibility: Pointer event intake, queued dispatch, and gesture state.

use crate::foundation::Point;
use crate::input::{PointerEvent, PointerOutcome, PointerRouter};
use crate::runtime::FrameScheduler;

/// Owns the pointer router and the queue of events waiting for the next frame.
///
/// Split out of `Engine` because this is a self-contained concern with its own state and its own
/// timing rule: events are queued rather than dispatched inline so a handler's signal writes land
/// on the frame boundary, and pointer history advances on the frame's own (already scaled) delta
/// so a gesture measured in headless stepping sees the velocity it would in the app. Folding that
/// into the engine mixed an event queue with frame orchestration and with the clock.
pub struct PointerInput {
    router: PointerRouter,
    /// Events received since the last dispatch, drained at the top of the next frame.
    queued: Vec<PointerEvent>,
}

impl PointerInput {
    /// Creates an input surface with no pointer state.
    pub fn new() -> Self {
        Self {
            router: PointerRouter::new(),
            queued: Vec::new(),
        }
    }

    /// Queues an event for dispatch at the start of the next frame.
    pub fn queue(&mut self, event: PointerEvent) {
        self.queued.push(event);
    }

    /// Queues a press at a point.
    pub fn queue_down(&mut self, x: f32, y: f32) {
        self.queue(simple_event(x, y, crate::input::PointerPhase::Down));
    }

    /// Queues a release at a point.
    pub fn queue_up(&mut self, x: f32, y: f32) {
        self.queue(simple_event(x, y, crate::input::PointerPhase::Up));
    }

    /// Queues a move, classified as drag or hover by what the router already recognises.
    ///
    /// Classification reads the router's current gesture rather than any local flag, so a move
    /// that arrives before the press was dispatched is still classified consistently with the
    /// gesture the router ends up running.
    pub fn queue_move(&mut self, x: f32, y: f32) {
        let phase = if self.router.is_dragging() {
            crate::input::PointerPhase::Drag
        } else {
            crate::input::PointerPhase::Move
        };
        self.queue(simple_event(x, y, phase));
    }

    /// Pointer position last seen, absent while the pointer is outside the window.
    pub fn position(&self) -> Option<Point> {
        self.router.cursor()
    }

    /// Whether the pointer has travelled far enough past a press to count as a drag.
    ///
    /// A press alone is not a drag, and treating it as one is how "the card is stuck to the
    /// cursor" bugs get introduced.
    pub fn is_dragging(&self) -> bool {
        self.router.is_dragging()
    }

    /// Releases any gesture being held, for instance on focus loss or pointer-leave.
    ///
    /// This gives up the capture as well as the hover. A drag that is only half-released keeps
    /// sending moves to the view it was pressed on, so the view follows a pointer that no longer
    /// exists.
    pub fn cancel_gesture(&mut self) {
        self.router.cancel_gesture();
    }

    /// Drains the queue, returning the state left behind by the last event dispatched.
    ///
    /// Returns `None` without advancing the clock when nothing is queued, so an idle frame does
    /// not age gesture history on its own.
    pub fn dispatch(
        &mut self,
        scheduler: &FrameScheduler,
        last_delta: f32,
    ) -> Option<PointerOutcome> {
        if self.queued.is_empty() {
            return None;
        }
        // Pointer history advances on the frame's own delta, so a gesture measured in headless
        // stepping sees exactly the velocity it would in the app.
        self.router.advance_clock(last_delta);
        // The arena is borrowed immutably for the whole batch: handlers write signals, and
        // signals only mark subscribers, so no node can be inserted or removed mid-dispatch.
        let mut last = None;
        for event in self.queued.drain(..) {
            last = Some(
                self.router
                    .dispatch(&scheduler.arena, scheduler.root, &event),
            );
        }
        last
    }

    /// Read-only access to the underlying router, for callers that need the full surface.
    pub fn router(&self) -> &PointerRouter {
        &self.router
    }

    /// Mutable access to the underlying router.
    pub fn router_mut(&mut self) -> &mut PointerRouter {
        &mut self.router
    }
}

impl Default for PointerInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a positioned event for a phase, with the primary button and no modifiers.
fn simple_event(x: f32, y: f32, phase: crate::input::PointerPhase) -> PointerEvent {
    PointerEvent {
        position: Point::new(x, y),
        button: crate::input::PointerButton::Primary,
        phase,
        click_count: 0,
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    }
}
