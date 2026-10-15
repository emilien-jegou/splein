// Single responsibility: Reactive signal adapters turning target changes into animated values.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::anim::ease::Ease;
use crate::anim::spring::Spring;
use crate::anim::track::ScalarTrack;
use crate::anim::tween::Tween;
use crate::reactive::readable::Readable;
use crate::reactive::runtime::ReactiveRuntime;
use crate::reactive::signal::Signal;
use crate::reactive::ticker::Ticker;

/// How a playback runs once it reaches its target.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    /// Holds the target value and stops.
    #[default]
    Once,
    /// Runs to the target, then back to the start, forever.
    PingPong,
    /// Jumps back to the start and runs again, forever.
    Loop,
}

/// Lifecycle of a controllable animation.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum PlayState {
    /// Advancing on every frame.
    #[default]
    Playing,
    /// Holding its value while retaining progress.
    Paused,
    /// Reached the end of a non-repeating run.
    Finished,
}

/// How a transition produces values over time.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Mode {
    /// Fixed-duration interpolation under an easing curve.
    Timed(Tween),
    /// Physics-based settling that retains velocity across retargets.
    Spring(Spring),
}

impl Mode {
    /// The spring whose parameters govern this mode's settling.
    ///
    /// A tween has no spring of its own, so it integrates with the default one; the result is
    /// never used, because a tween's value comes from its easing curve rather than its track.
    /// The method exists so a caller holding a `TransitionCell` can seed a track without caring
    /// which mode the cell was built in.
    pub fn spring(&self) -> Spring {
        match self {
            Mode::Spring(spring) => *spring,
            Mode::Timed(_) => Spring::default(),
        }
    }
}

/// A declared animation, reusable across any number of signals.
///
/// `on` adapts a target signal into an animated signal: the target says where to go, the
/// transition decides how, and the frame loop supplies the time. Nothing about the
/// transition reaches into the UI tree, so the same value drives layout, paint, or
/// anything else the caller binds it to.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Transition {
    mode: Mode,
    repeat: Repeat,
    from: Option<f32>,
    to: Option<f32>,
}

impl Transition {
    /// Declares a fixed-duration transition under an easing curve.
    pub const fn ease(duration: f32, ease: Ease) -> Self {
        Self {
            mode: Mode::Timed(Tween::new(duration, ease)),
            repeat: Repeat::Once,
            from: None,
            to: None,
        }
    }

    /// Declares a physics-based transition that keeps velocity when interrupted.
    pub const fn spring(spring: Spring) -> Self {
        Self {
            mode: Mode::Spring(spring),
            repeat: Repeat::Once,
            from: None,
            to: None,
        }
    }

    /// The obvious spring: snappy, no overshoot. `Transition::spring(Spring::snappy())`.
    pub const fn snappy() -> Self {
        Self::spring(Spring::snappy())
    }

    /// The obvious spring: soft landing with overshoot. `Transition::spring(Spring::bouncy())`.
    pub const fn bouncy() -> Self {
        Self::spring(Spring::bouncy())
    }

    /// The obvious spring: critically-damped glide. `Transition::spring(Spring::smooth())`.
    pub const fn smooth() -> Self {
        Self::spring(Spring::smooth())
    }

    /// Sets what happens once a self-driven playback reaches its target.
    pub const fn repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = repeat;
        self
    }

    /// Sets the starting value of a self-driven playback.
    pub const fn from(mut self, from: f32) -> Self {
        self.from = Some(from);
        self
    }

    /// Sets the target value of a self-driven playback.
    pub const fn to(mut self, to: f32) -> Self {
        self.to = Some(to);
        self
    }

    /// Parses a transition literal, so motion reads like any other prop:
    /// `Transition::from_str("spring")`, `.x(Transition::from_str("ease-out 300ms").on(target))`.
    ///
    /// Grammar (case-insensitive):
    /// - `"spring"` / `"spring:<preset>"` / `"spring <preset>"` — physics
    ///   (`snappy` default, `bouncy`, `smooth`, or `stiffness damping [mass]`)
    /// - `"<ease> <duration>"` / `"<duration> <ease>"` — timed, e.g.
    ///   `"ease-out 300ms"`, `"0.3s linear"`
    /// - `"<duration>"` alone — timed with the default (`out-cubic`) ease
    ///
    /// Durations: `300ms`, `0.3s`, or a bare number (seconds).
    ///
    /// Named `from_str` (not `from`) because the inherent playback builder
    /// `from()` would shadow `From::from` at the call site.
    ///
    /// This is the *secondary*, data-driven form — prefer the typed builders
    /// (`Transition::ease`, `Transition::spring`, `Transition::snappy()` /
    /// `::bouncy()` / `::smooth()`), which fail at compile time on typos.
    /// An unparseable string **panics in debug builds** (naming the offending
    /// string) and degrades to a snappy spring in release (same no-crash
    /// contract as foundation literals) while logging loudly.
    pub fn from_str(s: &str) -> Self {
        match Self::parse(s) {
            Some(t) => t,
            None => {
                let msg = format!("riode: invalid transition `{s}`; expected `spring[:preset]`, `<ease> <duration>`, or `<duration>`; using snappy spring");
                #[cfg(debug_assertions)]
                panic!("{msg}");
                #[cfg(not(debug_assertions))]
                {
                    tracing::error!("{msg}");
                    Self::spring(Spring::snappy())
                }
            }
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let lower = s.to_lowercase();
        if lower == "spring" {
            return Some(Self::spring(Spring::snappy()));
        }
        if let Some(rest) = lower
            .strip_prefix("spring:")
            .or_else(|| lower.strip_prefix("spring "))
        {
            return Spring::parse(rest.trim()).map(Self::spring);
        }
        // Timed: split ease and duration in either order.
        let parts: Vec<&str> = lower.split_whitespace().collect();
        let dur = |p: &str| {
            if let Some(ms) = p.strip_suffix("ms") {
                ms.trim().parse::<f32>().ok().map(|v| v / 1000.0)
            } else if let Some(sec) = p.strip_suffix('s') {
                sec.trim().parse::<f32>().ok()
            } else {
                p.parse::<f32>().ok()
            }
        };
        match parts.as_slice() {
            [single] => {
                if let Some(d) = dur(single) {
                    return Some(Self::ease(d, Ease::default()));
                }
                // A bare ease name with an implicit 300ms.
                if let Some(e) = Ease::parse(single) {
                    return Some(Self::ease(0.3, e));
                }
                None
            }
            [a, b] => {
                if let (Some(d), Some(e)) = (dur(a), Ease::parse(b)) {
                    return Some(Self::ease(d, e));
                }
                if let (Some(e), Some(d)) = (Ease::parse(a), dur(b)) {
                    return Some(Self::ease(d, e));
                }
                None
            }
            _ => None,
        }
    }

    /// Duration a playback run takes, or `None` when this transition is a spring.
    fn playback_duration(&self) -> Option<f32> {
        match self.mode {
            Mode::Timed(tween) => Some(tween.duration.max(f32::EPSILON)),
            Mode::Spring(_) => None,
        }
    }

    /// Animates a numeric signal, returning a signal holding the current value.
    ///
    /// The output starts at the target, so only later changes animate. A spring keeps its
    /// live velocity across target changes, which is what makes interruption seamless.
    ///
    /// Prefer [`use_transition`](crate::reactive::use_transition) inside components,
    /// especially rows in reactive lists: it recalls the cell across rebuilds so a
    /// retarget keeps position *and* velocity, and it registers the frame ticker once
    /// per hook slot instead of once per rebuild. Use `on` only for one-shot,
    /// non-component bindings where no rebuild can re-run the call.
    pub fn on(self, target: impl Readable<f32> + 'static) -> Signal<f32> {
        let reader: Rc<dyn Readable<f32>> = Rc::from(Box::new(target) as Box<dyn Readable<f32>>);
        let cell = Rc::new(TransitionCell::new(self.mode, Rc::clone(&reader)));
        reader.attach(Box::new(TargetTicker(Rc::clone(&cell))));
        cell.out()
    }

    /// Builds the state a recalled transition would keep across a rebuild.
    ///
    /// Pair with [`TransitionCell::retarget_source`]. Used by
    /// [`use_transition`](crate::reactive::use_transition); exposed because a cell cannot be
    /// assembled outside this module and the hook needs to make one.
    pub fn cell(self, target: impl Readable<f32> + 'static) -> Rc<TransitionCell> {
        let reader: Rc<dyn Readable<f32>> = Rc::from(Box::new(target) as Box<dyn Readable<f32>>);
        Rc::new(TransitionCell::new(self.mode, reader))
    }

    /// Animates a self-driving playback between two values, with a control handle.
    ///
    /// This is the one shape `on` cannot express: there is no target signal to watch,
    /// because the animation is its own source of truth.
    pub fn playback(
        self,
        runtime: &Rc<RefCell<ReactiveRuntime>>,
        from: f32,
        to: f32,
    ) -> (Signal<f32>, ControllerHandle) {
        let (from, to) = (self.from.unwrap_or(from), self.to.unwrap_or(to));
        let control = Rc::new(RefCell::new(Control {
            state: PlayState::Playing,
            restart: false,
            seek: None,
        }));
        let out = Signal::new(Rc::clone(runtime), from);
        let handle = ControllerHandle {
            control: Rc::clone(&control),
            out: out.clone(),
            runtime: Rc::clone(runtime),
        };
        runtime
            .borrow_mut()
            .register_ticker(Box::new(PlaybackTicker {
                out,
                control,
                duration: self.playback_duration().unwrap_or(1.0),
                from,
                to,
                ease: match self.mode {
                    Mode::Timed(tween) => tween.ease,
                    Mode::Spring(_) => Ease::Linear,
                },
                repeat: self.repeat,
                reverse: false,
                elapsed: 0.0,
            }));
        (handle.output(), handle)
    }
}

impl std::str::FromStr for Transition {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Self::parse(s).ok_or_else(|| {
            format!("invalid transition `{s}`; expected `spring[:preset]`, `<ease> <duration>`, or `<duration>` (e.g. `spring`, `spring:bouncy`, `ease-out 300ms`)")
        })
    }
}

impl From<&str> for Transition {
    fn from(s: &str) -> Self {
        // NOTE: call as `Transition::from_str(s)`; the inherent playback
        // builder `from()` shadows `From::from` in `Transition::from(..)` syntax.
        Self::from_str(s)
    }
}

/// Mutable playback state shared between a handle and its ticker.
#[derive(Debug)]
struct Control {
    state: PlayState,
    restart: bool,
    seek: Option<f32>,
}

/// Handle controlling a self-driven playback from outside the reactive graph.
#[derive(Clone)]
pub struct ControllerHandle {
    control: Rc<RefCell<Control>>,
    out: Signal<f32>,
    runtime: Rc<RefCell<ReactiveRuntime>>,
}

impl ControllerHandle {
    /// Signal carrying the playback's current value.
    pub fn output(&self) -> Signal<f32> {
        self.out.clone()
    }

    /// Current lifecycle state.
    pub fn state(&self) -> PlayState {
        self.control.borrow().state
    }

    /// Suspends playback, retaining progress.
    pub fn pause(&self) {
        self.control.borrow_mut().state = PlayState::Paused;
        self.wake();
    }

    /// Resumes playback from a pause.
    pub fn resume(&self) {
        let mut control = self.control.borrow_mut();
        if control.state == PlayState::Paused {
            control.state = PlayState::Playing;
        }
        drop(control);
        self.wake();
    }

    /// Jumps back to the start and plays again.
    pub fn restart(&self) {
        {
            let mut control = self.control.borrow_mut();
            control.restart = true;
            control.state = PlayState::Playing;
        }
        self.wake();
    }

    /// Scrubs to a normalized progress in `0.0..=1.0`.
    pub fn seek(&self, fraction: f32) {
        self.control.borrow_mut().seek = Some(fraction.clamp(0.0, 1.0));
        self.wake();
    }

    /// Requests a frame so a state change made while idle is observed.
    fn wake(&self) {
        self.runtime.borrow_mut().wake();
    }
}

/// Advances an animated signal toward whatever its target signal currently says.
struct TargetTicker(Rc<TransitionCell>);

impl Ticker for TargetTicker {
    fn advance(&mut self, dt: f32) -> bool {
        self.0.advance(dt);
        // Always keep the ticker: the target may move again, and this is what would notice.
        true
    }

    fn is_active(&self) -> bool {
        !self.0.settled()
    }
}

/// The state of one transition, shared between its ticker and whatever owns it.
///
/// The reason this is a reference-counted cell rather than fields inside the ticker: two owners
/// need it. The ticker advances it once a frame, and a component's hook state holds it across
/// rebuilds. While the ticker owned this state outright, a rebuilt component could only ever
/// receive a *new* transition sitting exactly on its target — which is how a FLIP silently
/// degrades into a teleport, because `animated - current` is zero on the frame the move lands.
///
/// Splitting the cell out is also what stops the ticker list growing without bound. A cell is
/// created once per hook slot and re-registered at most once, rather than one ticker being
/// appended on every rebuild.
pub struct TransitionCell {
    /// What this transition watches. Swappable, because a component's derived target is a fresh
    /// object on every rebuild while the cell that watches it is not.
    target: RefCell<Rc<dyn Readable<f32>>>,
    out: Signal<f32>,
    mode: Mode,
    track: RefCell<ScalarTrack>,
    last_goal: Cell<f32>,
    segment_from: Cell<f32>,
    elapsed: Cell<f32>,
    /// Whether the last frame still had somewhere to go.
    ///
    /// A transition outlives its own motion: the target can change again at any time, so the
    /// cell is never retired. What it can do is *report* honestly when it has arrived, so the
    /// engine stops asking for frames while everything sits at rest.
    settled: Cell<bool>,
}

impl TransitionCell {
    /// Creates a cell already sitting on its target's current value.
    pub fn new(mode: Mode, target: Rc<dyn Readable<f32>>) -> Self {
        let initial = target.read();
        Self {
            out: Signal::new(target.owner(), initial),
            target: RefCell::new(target),
            track: RefCell::new(ScalarTrack::new(mode.spring(), initial, initial)),
            mode,
            last_goal: Cell::new(initial),
            segment_from: Cell::new(initial),
            elapsed: Cell::new(0.0),
            settled: Cell::new(true),
        }
    }

    /// The animated value.
    pub fn out(&self) -> Signal<f32> {
        self.out.clone()
    }

    /// Points this transition at a new source.
    pub fn retarget_source(&self, target: Rc<dyn Readable<f32>>) {
        *self.target.borrow_mut() = target;
    }

    /// Whether the last advance arrived.
    pub fn settled(&self) -> bool {
        self.settled.get()
    }

    /// Integrates one frame toward the target's current value.
    pub fn advance(&self, dt: f32) {
        let target = self.target.borrow();
        let goal = target.read();
        let value = match self.mode {
            Mode::Spring(_) => {
                let mut track = self.track.borrow_mut();
                // A gesture hand-off is single-shot: the write that carried it is the one
                // gesture ending, so it is consumed here and not re-applied next frame.
                //
                // Read from the output as well as the target, because a component driving a
                // transition from a *derived* target cannot write a velocity onto that
                // derivation -- only a signal carries one -- so the animated value itself has
                // to be able to accept the launch. The output wins when both are present.
                if let Some(velocity) = self.out.take_motion_handoff() {
                    track.adopt_velocity(velocity);
                }
                if let Some(velocity) = target.take_motion_handoff() {
                    track.adopt_velocity(velocity);
                }
                // Retargeting every frame is idempotent and keeps interruption free.
                track.retarget(goal);
                track.advance(dt);
                self.settled.set(track.is_settled());
                if track.is_settled() {
                    goal
                } else {
                    track.position
                }
            }
            Mode::Timed(tween) => {
                if goal != self.last_goal.get() {
                    // Restart the segment from wherever the value currently is.
                    self.last_goal.set(goal);
                    self.segment_from.set(self.out.get());
                    self.elapsed.set(0.0);
                }
                self.elapsed.set(self.elapsed.get() + dt);
                self.settled.set(tween.is_complete(self.elapsed.get()));
                tween.lerp(self.elapsed.get(), self.segment_from.get(), goal)
            }
        };
        drop(target);
        self.out.set_if_changed(value);
    }
}

/// Advances a self-driven playback, honouring the control handle each frame.
struct PlaybackTicker {
    out: Signal<f32>,
    control: Rc<RefCell<Control>>,
    duration: f32,
    from: f32,
    to: f32,
    ease: Ease,
    repeat: Repeat,
    reverse: bool,
    elapsed: f32,
}

impl PlaybackTicker {
    /// Eased progress in `0.0..=1.0` along the current direction.
    fn progress(&self) -> f32 {
        let raw = (self.elapsed / self.duration).clamp(0.0, 1.0);
        let eased = self.ease.sample(raw);
        if self.reverse {
            1.0 - eased
        } else {
            eased
        }
    }

    /// Advances the clock, returning whether the run is still alive.
    fn step(&mut self, dt: f32) -> bool {
        self.elapsed += dt;
        if self.elapsed < self.duration {
            return true;
        }
        match self.repeat {
            Repeat::Once => false,
            Repeat::Loop => {
                self.elapsed %= self.duration;
                self.reverse = false;
                true
            }
            Repeat::PingPong => {
                self.elapsed %= self.duration;
                self.reverse = !self.reverse;
                true
            }
        }
    }
}

impl Ticker for PlaybackTicker {
    fn advance(&mut self, dt: f32) -> bool {
        let (mut state, restart, seek) = {
            let mut control = self.control.borrow_mut();
            let taken = (
                control.state,
                std::mem::take(&mut control.restart),
                control.seek.take(),
            );
            control.state = taken.0;
            taken
        };

        if restart {
            self.elapsed = 0.0;
            self.reverse = false;
        }
        if let Some(fraction) = seek {
            self.elapsed = fraction * self.duration;
            state = PlayState::Playing;
            self.control.borrow_mut().state = state;
        }
        if state == PlayState::Paused {
            return true;
        }

        let alive = self.step(dt);
        if !alive {
            self.control.borrow_mut().state = PlayState::Finished;
        }
        let value = self.from + (self.to - self.from) * self.progress();
        self.out.set_if_changed(value);
        alive
    }

    fn is_active(&self) -> bool {
        self.control.borrow().state == PlayState::Playing
    }
}
