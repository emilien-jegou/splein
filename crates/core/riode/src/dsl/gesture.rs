// Single responsibility: the Drag gesture primitive — one object owning the
// grab/hold/track/release state machine every draggable otherwise hand-rolls
// across five handlers (`motion_reorder/row.rs` was ~76 lines of it).
//
// A drag is press → moves → release with a velocity at the end. The gesture
// owns that lifecycle (grab point, held flag, live position, release
// cleanup); the client keeps owning its motion signals and puts domain logic
// (reorder swaps, glide seeding) in the step/end callbacks, which receive the
// full `DragInfo` including release velocity for spring hand-off.

use std::rc::Rc;

use crate::dsl::group::{Behavior, GroupDef};
use crate::foundation::geometry::Point;
use crate::input::event::{DragInfo, PointerEvent};
use crate::reactive::{use_signal, Signal};

/// Creates the drag-position signal a [`Drag`] drives: `Some(pos)` from
/// drag-start through release, `None` at rest — including between press and
/// drag-start, when the press is not a drag yet. Stable across rebuilds.
pub fn use_gesture() -> Signal<Option<Point>> {
    use_signal(None)
}

/// A drag gesture: grab offset, held flag, live position, and release.
///
/// Built by [`Drag::start`] and attached with `GroupDef::on_drag` (RSX
/// `on:drag={…}`). The builder fans out to the five typed slots, so call
/// sites read as one verb instead of five closures — and every `on:drag` node
/// is drag-gated by construction (the trio slots are always set), with release
/// cleanup guaranteed even when the client sets no callbacks.
///
/// `Drag::start` must run unconditionally during a component body: `held` and
/// `grab` allocate hook state in call order, like any hook.
pub struct Drag {
    at: Signal<Option<Point>>,
    held: Signal<bool>,
    grab: Signal<Point>,
    on_grab: Option<Rc<dyn Fn(&PointerEvent)>>,
    on_step: Option<Rc<dyn Fn(&DragInfo)>>,
    on_end: Option<Rc<dyn Fn(&DragInfo)>>,
}

impl Drag {
    /// Begins a drag bound to `at` (usually from [`use_gesture`]): drag-start
    /// sets `Some(pos)` and moves update it; release resets to `None`. Press
    /// alone latches nothing — tracking starts when the press becomes a drag.
    pub fn start(at: Signal<Option<Point>>) -> Self {
        Self {
            at,
            held: use_signal(false),
            grab: use_signal(Point::ZERO),
            on_grab: None,
            on_step: None,
            on_end: None,
        }
    }

    /// Runs on press, after grab/held/position are recorded.
    pub fn grab(mut self, f: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_grab = Some(Rc::new(f));
        self
    }

    /// Runs on drag-start (as the first step) and every drag-step.
    pub fn step(mut self, f: impl Fn(&DragInfo) + 'static) -> Self {
        self.on_step = Some(Rc::new(f));
        self
    }

    /// Runs on release, after position clears. `info.velocity` is the hand-off
    /// for a spring — what stops a flick from appearing to stop dead.
    pub fn end(mut self, f: impl Fn(&DragInfo) + 'static) -> Self {
        self.on_end = Some(Rc::new(f));
        self
    }

    /// Whether a press is currently held on this gesture.
    pub fn held(&self) -> Signal<bool> {
        self.held
    }

    /// Where the press landed, in window coordinates.
    pub fn grab_point(&self) -> Signal<Point> {
        self.grab
    }
}

impl Behavior for Drag {
    fn attach(self, row: GroupDef) -> GroupDef {
        let Self {
            at,
            held,
            grab,
            on_grab,
            on_step,
            on_end,
        } = self;
        // One Rc per callback; each of the five closures holds its own clone.
        let grab_cb = on_grab;
        let step_cb = on_step;
        let end_cb = on_end;
        let step_start = step_cb.clone();
        row.on_pointer_down(move |e: &PointerEvent| {
            grab.set(e.position);
            held.set(true);
            // NOTE: position tracking deliberately starts at drag-start, not
            // here. A press is not a drag yet — latching `Some` now would
            // engage followers with the full grab offset on the first held
            // frame (a visible jump), where the pre-gesture code waited for
            // drag-start (~4px in) and eased in smoothly.
            if let Some(g) = grab_cb.as_ref() {
                g(e);
            }
        })
        .on_drag_start(move |info: &DragInfo| {
            at.set(Some(info.position));
            if let Some(s) = step_start.as_ref() {
                s(info);
            }
        })
        .on_drag_step({
            let at = at;
            let step_cb = step_cb;
            move |info: &DragInfo| {
                at.set(Some(info.position));
                if let Some(s) = step_cb.as_ref() {
                    s(info);
                }
            }
        })
        .on_drag_end(move |info: &DragInfo| {
            // The end callback runs FIRST, while the displacement is still
            // live: it typically seeds a release glide from the current
            // offset, which reads as zero once cleared below (the row would
            // snap to its slot instead of gliding home).
            if let Some(e) = end_cb.as_ref() {
                e(info);
            }
            at.set(None);
            held.set(false);
        })
        // Safety clear: a press that releases without ever becoming a drag
        // fires no drag-end, but must still leave no stale hold behind.
        .on_pointer_up(move |_: &PointerEvent| {
            at.set(None);
            held.set(false);
        })
    }
}
