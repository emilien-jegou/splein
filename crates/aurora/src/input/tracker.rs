// Single responsibility: Pointer gesture state: capture, drag recognition, and release velocity.

use crate::foundation::Point;
use crate::input::event::{DragInfo, PointerButton, PointerEvent, PointerPhase};
use crate::tree::NodeId;

/// Pointer travel, in logical pixels, before a press becomes a drag rather than a click.
pub const DRAG_THRESHOLD: f32 = 4.0;

/// How much of the recent pointer history release velocity is averaged over.
///
/// A short window tracks a flick; a long one drags in the throwaway velocity of the moment
/// the pointer stopped, which is not what the user meant.
const VELOCITY_WINDOW_SECS: f32 = 0.09;

/// Idle time after which a click burst expires, so two slow clicks are two single clicks.
const CLICK_BURST_SECS: f32 = 0.4;

/// One sample of pointer history used to reconstruct release velocity.
#[derive(Copy, Clone, Debug)]
struct Sample {
    position: Point,
    at: f32,
}

/// A pointer sample plus the gesture state it implies.
pub struct PointerTracker {
    /// Latest known cursor position, absent while the pointer is outside the window.
    pub cursor: Option<Point>,
    /// Whether a button is currently held.
    held: bool,
    /// Node that received the press, which keeps receiving moves and the release.
    ///
    /// Capture is what makes a drag survive the pointer leaving the view it started on.
    capture: Option<NodeId>,
    /// Position where the press began.
    press_at: Point,
    /// Position of the most recent sample.
    last: Point,
    /// Whether the current press may become a drag at all.
    ///
    /// Set per press by the router from the press chain: a press on content with no
    /// drag handlers is a press no matter how far it moves — the web draws the same
    /// line (a `dragstart` only exists for draggable content), and without it every
    /// click that drifts past the threshold dies silently with no handler observing
    /// anything. Defaults to true so a bare tracker behaves as before.
    drag_enabled: bool,
    /// Whether the gesture has passed the drag threshold.
    dragging: bool,
    /// Whether a click should be delivered when the press is released.
    ///
    /// Cleared by any movement, so a drag never also fires a click at the end.
    click_armed: bool,
    /// Clicks observed in the current burst, for double-click detection.
    clicks: u32,
    /// Seconds since the previous click, used to expire the burst.
    since_click: f32,
    /// Engine time of the most recent sample.
    last_sample_at: f32,
    history: Vec<Sample>,
}

impl PointerTracker {
    /// Creates a tracker with no pointer contact.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the current press may be recognised as a drag.
    ///
    /// The router sets this per press from the press chain. Test and harness code
    /// driving the tracker directly keeps the default (enabled).
    pub fn set_drag_enabled(&mut self, enabled: bool) {
        self.drag_enabled = enabled;
    }

    /// Node currently capturing the gesture.
    #[inline]
    pub fn capture(&self) -> Option<NodeId> {
        self.capture
    }

    /// Whether a gesture is in progress, past or short of the drag threshold.
    #[inline]
    pub fn is_held(&self) -> bool {
        self.held
    }

    /// Whether the gesture has been recognised as a drag.
    #[inline]
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Whether releasing now would deliver a click.
    #[inline]
    pub fn click_armed(&self) -> bool {
        self.click_armed
    }

    /// Pointer velocity over the recent history, in logical pixels per second.
    ///
    /// Derived from engine-supplied timestamps rather than the wall clock so a headless test
    /// stepping fixed deltas measures the same velocity the app would.
    pub fn velocity(&self) -> Point {
        if self.history.len() < 2 {
            return Point::ZERO;
        }
        let newest = *self.history.last().expect("checked length");
        // The *oldest* sample still inside the window, which is the whole span the flick
        // covers. Falling back to the oldest available keeps a single slow drag from
        // reporting zero velocity.
        let oldest = self
            .history
            .iter()
            .find(|s| newest.at - s.at <= VELOCITY_WINDOW_SECS)
            .copied()
            .unwrap_or_else(|| *self.history.first().expect("checked length"));
        let dt = newest.at - oldest.at;
        if dt <= f32::EPSILON {
            return Point::ZERO;
        }
        Point::new(
            (newest.position.x - oldest.position.x) / dt,
            (newest.position.y - oldest.position.y) / dt,
        )
    }

    /// Current drag payload, for handing to a gesture callback.
    pub fn drag_info(&self, position: Point, held: bool) -> DragInfo {
        DragInfo {
            position,
            delta: Point::new(position.x - self.press_at.x, position.y - self.press_at.y),
            velocity: self.velocity(),
            held,
        }
    }

    /// Folds one pointer sample into the tracker at engine time `at`, returning what changed.
    pub fn feed(&mut self, event: &PointerEvent, at: f32) -> TrackerChange {
        // A burst expires on its own so a slow second click is a fresh single click.
        if self.clicks > 0 && at - self.last_sample_at > CLICK_BURST_SECS {
            self.clicks = 0;
        }
        self.last_sample_at = at;
        self.cursor = Some(event.position);
        self.push_sample(event.position, at);

        match event.phase {
            PointerPhase::Down => {
                self.held = true;
                self.dragging = false;
                self.click_armed = true;
                self.press_at = event.position;
                self.last = event.position;
                self.history.clear();
                self.push_sample(event.position, at);
                TrackerChange::Pressed
            }
            PointerPhase::Move | PointerPhase::Drag => {
                // The threshold is checked in both phases. A platform can report a held
                // button as a drag-bearing sample without ever delivering a plain move, and
                // a gesture must still be recognised when the finger simply leaves the view.
                if self.held {
                    let travel = (event.position.x - self.press_at.x).abs()
                        + (event.position.y - self.press_at.y).abs();
                    // Gated on drag interest as well as distance: a press on content
                    // with no drag handlers never becomes a drag, however far it moves.
                    // Without this, every click that drifts past the threshold dies —
                    // no click, and no drag handler observing anything either.
                    if self.drag_enabled && !self.dragging && travel > DRAG_THRESHOLD {
                        self.dragging = true;
                        // Movement disqualifies the press as a click, so releasing after a
                        // drag cannot also fire one.
                        self.click_armed = false;
                        self.last = event.position;
                        return TrackerChange::DragStarted;
                    }
                }
                self.last = event.position;
                // A drag-phase sample below the threshold is not a drag step: some
                // platforms report any held movement as drag-bearing, and treating a
                // 1px wiggle as a drag would both move the view and (via click_armed
                // still holding) deliver a click for the same press. Only a recognised
                // drag streams steps; anything else is an ordinary move.
                if event.phase == PointerPhase::Drag && self.dragging {
                    TrackerChange::Dragged
                } else {
                    TrackerChange::Moved
                }
            }
            PointerPhase::Up => {
                self.held = false;
                let was_dragging = self.dragging;
                self.dragging = false;
                self.capture = None;
                self.last = event.position;
                if self.click_armed {
                    self.clicks = self.clicks.saturating_add(1).min(2);
                    self.since_click = 0.0;
                    self.click_armed = false;
                    return TrackerChange::Released { click: self.clicks };
                }
                if was_dragging {
                    TrackerChange::DragEnded
                } else {
                    TrackerChange::Released { click: 0 }
                }
            }
        }
    }

    /// Appends a sample, discarding history older than the velocity window.
    fn push_sample(&mut self, position: Point, at: f32) {
        self.history.push(Sample { position, at });
        if self.history.len() > 2 {
            let cutoff = at - VELOCITY_WINDOW_SECS;
            let drop = self
                .history
                .iter()
                .position(|s| s.at >= cutoff)
                .unwrap_or(self.history.len() - 1);
            if drop > 0 {
                self.history.drain(..drop);
            }
        }
    }

    /// Assigns the node that captured the press.
    #[inline]
    pub fn set_capture(&mut self, node: Option<NodeId>) {
        self.capture = node;
    }
}

impl Default for PointerTracker {
    fn default() -> Self {
        Self {
            cursor: None,
            held: false,
            capture: None,
            press_at: Point::ZERO,
            last: Point::ZERO,
            drag_enabled: true,
            dragging: false,
            click_armed: false,
            clicks: 0,
            since_click: 0.0,
            last_sample_at: 0.0,
            history: Vec::new(),
        }
    }
}

impl PointerTracker {
    /// Abandons the gesture in progress, leaving no pointer contact behind.
    ///
    /// Needed when the pointer goes away without a release arriving: the window losing focus,
    /// the cursor leaving the surface, a compositor grabbing the pointer. Dropping only the
    /// hover is not enough, because the capture and the drag flag survive it and the pressed
    /// view stays pinned to a position the pointer abandoned — a row that follows the cursor
    /// off the window and never comes back.
    ///
    /// The click burst is deliberately kept: a press that was abandoned should still not
    /// deliver a click, but a genuine double click on either side of a focus change is still a
    /// double click.
    pub fn cancel_gesture(&mut self) {
        self.held = false;
        self.drag_enabled = true;
        self.dragging = false;
        self.capture = None;
        self.click_armed = false;
        self.history.clear();
        self.cursor = None;
    }

    /// Press position, which a gesture measures its drag from.
    #[inline]
    pub fn press_origin(&self) -> Point {
        self.press_at
    }

    /// Button the current gesture concerns.
    #[inline]
    pub fn button(&self) -> PointerButton {
        PointerButton::Primary
    }
}

/// What a tracker change means for dispatch.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TrackerChange {
    /// A button went down; the hit chain should capture the topmost interested node.
    Pressed,
    /// The pointer moved with no button held.
    Moved,
    /// The press travelled far enough to become a drag.
    DragStarted,
    /// A drag is in progress.
    Dragged,
    /// The button came up, with the click count if one is owed.
    Released { click: u32 },
    /// A drag finished on release.
    DragEnded,
}
