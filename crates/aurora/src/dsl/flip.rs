// Single responsibility: FLIP as a client-side hook — measure the move, spring back from it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::anim::spring::Spring;
use crate::anim::track::ScalarTrack;
use crate::dsl::element_ref::ElementRef;
use crate::foundation::{Point, Transform};
use crate::reactive::hooks::{current_scope, next_slot, no_scope, recall_value, remember_value};
use crate::reactive::ticker::Ticker;
use crate::reactive::ReactiveRuntime;
use crate::reactive::{Effect, Readable, Signal};

/// Animates an element between the positions layout gives it, in two dimensions.
///
/// Binds a geometry handle to the element, watches a trigger for "the arrangement changed", and
/// returns the transform that carries it. Two lines at the call site, and — the part that is
/// usually wrong — it keeps working when the window changes shape.
///
/// # Why this is on the client
///
/// The FLIP technique is four steps: **F**irst position (where it was), **L**ast position (where
/// layout just put it), **I**nvert (jump the element back to First without animating), **P**lay
/// (animate that offset to zero). All four happen here. The engine lays out, publishes where
/// things went, and paints whatever transform it is handed; it has no notion of a move.
///
/// # Why the trigger exists
///
/// Position also changes when nothing was reordered. Squashing the window moves every row in the
/// list, and animating that would make a resize look like a shuffle. So the position alone is not
/// a reason to animate: the arrangement has to have changed too, and comparing the two is what
/// separates "this row swapped places" from "the whole list got shorter".
///
/// # The honest constraint
///
/// Reading geometry is a pull, not a notification, so this samples once per frame rather than
/// reacting to a measurement. It is therefore one frame behind the layout that moved the element.
/// In exchange, sampling cannot dirty a node, cannot schedule the frame that would invalidate the
/// sample, and cannot feed back into layout — a measurement that triggered layout would oscillate.
/// That trade is what makes animating a reorder safe at all.
///
/// # Lifetime
///
/// State lives in hook memory, so a rebuilt component finds its own. This is the reason it is a
/// hook and not a free function: a reactive list re-runs its builder on every reorder, and a fresh
/// spring per rebuild would sit on the new position and animate nothing.
pub fn use_flip(
    target: ElementRef,
    trigger: impl Readable<Trigger> + 'static,
    spring: Spring,
) -> Signal<Transform> {
    let Some(scope) = current_scope() else {
        no_scope()
    };
    let slot = next_slot(&scope);

    let cell = match scope
        .stable
        .then(|| recall_value::<Rc<FlipCell>>(&scope, slot))
        .flatten()
    {
        Some(existing) => existing,
        None => {
            let fresh = Rc::new(FlipCell::new(target, spring));
            if scope.stable {
                remember_value(&scope, slot, &fresh);
            }
            fresh
        }
    };

    // Two phases, deliberately.
    //
    // The trigger is noticed by an *effect*, which runs in the reactive drain before layout.
    // The sample and the integration happen in the *post-layout* phase, because both need the
    // boxes layout has just produced. Splitting them this way is what makes the phases line up:
    // the arrangement has changed by the time layout runs, and layout's answer is available by
    // the time the sample asks for it.
    // Stored in the cell rather than dropped here. `Effect` disposes itself on drop, so a
    // temporary would unsubscribe before the trigger ever moved and the FLIP would never fire —
    // which looks exactly like a hook that does nothing, with nothing to point at.
    //
    // Armed only on a genuine trigger change. `Effect::new` runs its action once at creation,
    // and this hook re-creates its effect on every rebuild — so an unconditional arm here
    // would go off on any rebuild that also moved boxes, animating a resize as if it were a
    // reorder and parking rows outside the window.
    cell.set_effect(Effect::new(cell.runtime(), {
        let cell = Rc::clone(&cell);
        move || {
            // Read so the effect subscribes to the trigger. The value itself is what matters:
            // only a *different* arrangement arms the next sample.
            let current = trigger.read();
            let previous = cell.last_trigger.replace(Some(current));
            if previous.is_some_and(|seen| seen != current) {
                cell.mark_moved();
            }
        }
    }));

    // Sampling and integrating, once per frame, against final geometry. Registered only on the
    // build that created the cell: a hook body runs again on every rebuild, and a callback
    // registered per rebuild would accumulate — 24 drags in `reorder_resize_stress` grew this
    // from 10 entries to 50.
    //
    // One `if`, deliberately: `register_callbacks_once` is true exactly once, so two separate
    // checks would leave the second body dead — the ticker that keeps the frame loop asking
    // while an offset settles would never attach, and a FLIP would freeze mid-flight the
    // moment nothing else was driving frames.
    if cell.register_callbacks_once() {
        cell.runtime().borrow_mut().register_post_layout({
            let cell = Rc::clone(&cell);
            move |dt| {
                if let Some(delta) = cell.sample() {
                    cell.invert(delta.0, delta.1);
                }
                cell.advance(dt);
            }
        });
        // The ticker does no work. It exists solely so the frame loop knows an offset is still
        // settling: `has_active_motion` is ticker-based, and a FLIP that animated without one
        // would finish on screen only if something else happened to keep asking for frames.
        cell.out.attach(Box::new(FlipTicker(Rc::clone(&cell))));
    }

    cell.out.clone()
}

/// Whether the arrangement a FLIP is watching has changed.
///
/// A revision rather than the arrangement itself: comparing a whole list of labels every frame
/// is work proportional to the list, and the only question is "is this a different arrangement
/// from the one whose positions I cached".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Trigger {
    /// Incremented by the owner whenever the arrangement changes.
    pub revision: u64,
}

impl From<u32> for Trigger {
    fn from(revision: u32) -> Self {
        Self {
            revision: revision as u64,
        }
    }
}

impl From<u64> for Trigger {
    fn from(revision: u64) -> Self {
        Self { revision }
    }
}

/// One element's FLIP state: the offset being animated, and the position it was last seen at.
struct FlipCell {
    target: ElementRef,
    out: Signal<Transform>,
    x: RefCell<ScalarTrack>,
    y: RefCell<ScalarTrack>,
    /// Where the element sat when this was last sampled.
    last: Cell<Option<Point>>,
    /// The arrangement last observed by the trigger effect. Compared rather than merely
    /// noticed, so recreating the effect on a rebuild that changed nothing does not arm a
    /// sample — without this, any rebuild that also moved boxes (a resize) would animate
    /// as if it were a reorder.
    last_trigger: Cell<Option<Trigger>>,
    /// Set when the arrangement changed and the next sample should be treated as a move.
    ///
    /// Cleared by the sample that consumes it, which is what keeps it from being sticky: a
    /// trigger that fires without moving anything must not leave a pending animation behind.
    owed: Cell<bool>,
    /// Whether an offset is currently being animated.
    running: Cell<bool>,
    /// Kept alive for its subscription; see `set_effect`.
    ///
    /// An `Effect` unsubscribes when dropped, so holding it here is what makes the trigger
    /// observation outlive the call that created it.
    effect: RefCell<Option<Effect>>,
    /// Whether this cell's tickers and post-layout callback have been registered.
    registered: Cell<bool>,
}

impl FlipCell {
    fn new(target: ElementRef, spring: Spring) -> Self {
        Self {
            out: Signal::new(target.runtime(), Transform::IDENTITY),
            x: RefCell::new(ScalarTrack::new(spring, 0.0, 0.0)),
            y: RefCell::new(spring_track(spring)),
            target,
            last: Cell::new(None),
            last_trigger: Cell::new(None),
            owed: Cell::new(false),
            running: Cell::new(false),
            effect: RefCell::new(None),
            registered: Cell::new(false),
        }
    }

    /// True exactly once, on the build that created this cell.
    ///
    /// A hook body runs again on every rebuild, and both a ticker and a post-layout callback are
    /// permanent once registered. Re-registering per rebuild would grow both structures without
    /// bound — which is why this is a flag on the cell rather than a check at the call site.
    fn register_callbacks_once(&self) -> bool {
        if self.registered.get() {
            return false;
        }
        self.registered.set(true);
        true
    }

    /// Holds the trigger subscription for as long as this cell lives.
    fn set_effect(&self, effect: Effect) {
        // Replaced rather than asserted absent: a rebuild recalls the same cell and hands it a
        // fresh effect, and the old one must go with it rather than accumulate.
        if let Some(mut previous) = self.effect.borrow_mut().take() {
            previous.dispose();
        }
        *self.effect.borrow_mut() = Some(effect);
    }

    fn runtime(&self) -> Rc<RefCell<ReactiveRuntime>> {
        self.target.runtime()
    }

    /// Notes that the arrangement changed.
    fn mark_moved(&self) {
        self.owed.set(true);
    }

    /// Samples the element and reports a delta worth inverting.
    ///
    /// Returns `None` for a resize — the element moved, but the arrangement did not — and for the
    /// first sighting, where there is no First position to invert from.
    fn sample(&self) -> Option<(f32, f32)> {
        let now = self.target.position();
        let previous = self.last.replace(now);
        // Consumed whatever the outcome: a trigger that fires without moving anything must not
        // leave this armed for some unrelated future sample.
        let owed = self.owed.replace(false);

        let (Some(now), Some(previous)) = (now, previous) else {
            return None;
        };

        // First - Last: the offset that puts the element back where it was, in the frame layout
        // just finished. Positive when it moved left or up.
        let dx = previous.x - now.x;
        let dy = previous.y - now.y;
        let moved = dx.abs() >= MOVE_EPSILON || dy.abs() >= MOVE_EPSILON;

        (owed && moved).then_some((dx, dy))
    }

    /// Inverts to `delta` and springs back to rest.
    fn invert(&self, dx: f32, dy: f32) {
        {
            let mut x = self.x.borrow_mut();
            x.position = dx;
            x.velocity = 0.0;
            x.retarget(0.0);
        }
        {
            let mut y = self.y.borrow_mut();
            y.position = dy;
            y.velocity = 0.0;
            y.retarget(0.0);
        }
        self.running.set(true);
        self.publish(dx, dy);
    }

    /// Advances both axes and republishes, until they are both at rest.
    fn advance(&self, dt: f32) {
        if !self.running.get() {
            return;
        }

        let dx = {
            let mut x = self.x.borrow_mut();
            x.advance(dt);
            x.position
        };
        let dy = {
            let mut y = self.y.borrow_mut();
            y.advance(dt);
            y.position
        };

        if self.x.borrow().is_settled() && self.y.borrow().is_settled() {
            // Snap to exactly zero rather than to wherever the integrator happened to stop. A
            // spring settles within a tolerance, and a residue that never reaches zero is a row
            // that is never quite in its slot.
            self.running.set(false);
            self.x.borrow_mut().position = 0.0;
            self.y.borrow_mut().position = 0.0;
            self.publish(0.0, 0.0);
        } else {
            self.publish(dx, dy);
        }
    }

    fn publish(&self, dx: f32, dy: f32) {
        self.out.set_if_changed(Transform::from_translation(dx, dy));
    }
}

/// A second track for the same spring, seeded at rest.
fn spring_track(spring: Spring) -> ScalarTrack {
    ScalarTrack::new(spring, 0.0, 0.0)
}

/// Creates a geometry handle that survives being rebuilt.
///
/// A plain `ElementRef::new` per build would work, and would also re-register the node every time
/// a list reordered — and, worse, would leave the FLIP comparing against a handle that had never
/// seen a position. Filed in hook memory, one handle per component for its whole life.
pub fn use_ref() -> ElementRef {
    let Some(scope) = current_scope() else {
        no_scope()
    };
    let slot = next_slot(&scope);
    if scope.stable {
        if let Some(existing) = recall_value::<ElementRef>(&scope, slot) {
            return existing;
        }
    }
    let runtime = Rc::clone(&scope.inner.borrow().runtime);
    let fresh = ElementRef::new(runtime);
    if scope.stable {
        remember_value(&scope, slot, &fresh);
    }
    fresh
}

/// A pointer-driven offset for an element, measured rather than accumulated.
///
/// The offset is "where the pointer wants this element's centre, minus the centre its box
/// already has". Those two move together when the window changes shape, so a resize mid-drag is
/// corrected by the next derivation instead of having to be noticed and compensated for — which
/// is the failure this replaces, where a distance accumulated from the grab stayed put while the
/// slot underneath it moved.
///
/// Recomputed in the post-layout phase for the same reason [`use_flip`] samples there: the box it
/// reads is the one layout has just produced.
pub fn use_follow(
    target: ElementRef,
    want_centre: impl Readable<Option<f32>> + 'static,
    grab_dy: impl Readable<f32> + 'static,
) -> Signal<Transform> {
    let Some(scope) = current_scope() else {
        no_scope()
    };
    let slot = next_slot(&scope);

    let cell = match scope
        .stable
        .then(|| recall_value::<Rc<FollowCell>>(&scope, slot))
        .flatten()
    {
        Some(existing) => existing,
        None => {
            let fresh = Rc::new(FollowCell::new(target, want_centre, grab_dy));
            if scope.stable {
                remember_value(&scope, slot, &fresh);
            }
            fresh
        }
    };

    // Follow needs a frame every time the pointer moves, and no ticker expresses that: the
    // pointer is not a signal. Registering one keeps the frame loop asking while a drag is live,
    // which is what stops a slow drag from stalling between pointer samples.
    if cell.register_callbacks_once() {
        cell.out.attach(Box::new(FollowTicker(Rc::clone(&cell))));
        cell.runtime().borrow_mut().register_post_layout({
            let cell = Rc::clone(&cell);
            move |_dt| cell.resolve()
        });
    }

    cell.out.clone()
}

/// One element's pointer-driven offset.
struct FollowCell {
    target: ElementRef,
    want_centre: Box<dyn Readable<Option<f32>>>,
    /// Retained for call-site compatibility; the gesture already folds the grab offset
    /// into `want_centre`, so resolving reads the desired centre directly.
    #[allow(dead_code)]
    grab_dy: Box<dyn Readable<f32>>,
    /// Whether a pointer is currently asking for this row, mirrored out of the signal.
    ///
    /// Mirrored rather than read because the frame loop asks "is anything still moving" from
    /// inside a borrow of the runtime, and reading the signal there would re-enter it.
    dragging: Cell<bool>,
    /// Whether this cell's ticker and post-layout callback have been registered.
    registered: Cell<bool>,
    out: Signal<Transform>,
}

impl FollowCell {
    fn new(
        target: ElementRef,
        want_centre: impl Readable<Option<f32>> + 'static,
        grab_dy: impl Readable<f32> + 'static,
    ) -> Self {
        Self {
            out: Signal::new(target.runtime(), Transform::IDENTITY),
            target,
            want_centre: Box::new(want_centre),
            grab_dy: Box::new(grab_dy),
            dragging: Cell::new(false),
            registered: Cell::new(false),
        }
    }

    /// True exactly once, on the build that created this cell. See `FlipCell`'s.
    fn register_callbacks_once(&self) -> bool {
        if self.registered.get() {
            return false;
        }
        self.registered.set(true);
        true
    }

    fn runtime(&self) -> Rc<RefCell<ReactiveRuntime>> {
        self.target.runtime()
    }

    /// Recomputes the offset from the current pointer intent and the current box.
    fn resolve(&self) {
        // `None` when nothing is held, which is what makes the offset fall to exactly zero on
        // release rather than to whatever the last pointer position implied.
        let Some(centre) = self.want_centre.read() else {
            // Released. Mirrored here rather than in the gesture so that the flag can only ever
            // disagree with the offset in one direction: released means the offset is zero.
            self.dragging.set(false);
            self.out.set_if_changed(Transform::IDENTITY);
            return;
        };
        self.dragging.set(true);
        let Some(rect) = self.target.rect() else {
            return;
        };

        // `centre` is already the desired centre in window coordinates: the gesture
        // subtracted the grab offset once, when it formed it.
        let want = centre;
        let have = rect.y + rect.height * 0.5;
        let ty = want - have;
        self.out
            .set_if_changed(Transform::from_translation(0.0, ty));
    }
}

/// Keeps the frame loop asking while a drag is live.
struct FollowTicker(Rc<FollowCell>);

impl Ticker for FollowTicker {
    fn advance(&mut self, _dt: f32) -> bool {
        true
    }

    fn is_active(&self) -> bool {
        // Active only while the pointer is asking for something. A follow that ran forever would
        // keep the redraw loop awake over a static list, which is the one cost a per-frame
        // measurement must not pay.
        //
        // Read through the cached `Cell`, not through `Readable::read`, because this is called
        // from inside a borrow of the runtime — `has_active_motion` reaches the ticker registry
        // through one — and a signal read borrows the same `RefCell` again. A `read` here would
        // re-enter a borrow that is already held, and it would also subscribe whatever observer
        // happened to be installed, which is not what this question is.
        self.0.dragging.get()
    }
}

/// Below this, a move is not worth animating.
///
/// Sub-pixel jitter is routine — fractional layout, then a rounding difference on the way to the
/// screen — and animating it puts a visible twitch on a list that is supposed to be sitting still.
/// A tenth of a pixel is well under anything a viewer can see.
const MOVE_EPSILON: f32 = 0.1;

/// Reports whether a FLIP offset is still settling, and does no work itself.
///
/// The distinction is the point: the sampling and integration happen post-layout, where the
/// geometry is final, while this only answers the frame loop's question of whether another frame
/// is worth drawing.
struct FlipTicker(Rc<FlipCell>);

impl Ticker for FlipTicker {
    fn advance(&mut self, _dt: f32) -> bool {
        true
    }

    fn is_active(&self) -> bool {
        self.0.running.get()
    }
}
