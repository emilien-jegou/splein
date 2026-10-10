// Single responsibility: A test harness owning an engine, its runtime, and pointer driving.

use std::cell::RefCell;
use std::rc::Rc;

use tiny_skia::Pixmap;

use crate::raster::{HeadlessFrame, HeadlessRaster};
use crate::testing::snapshot::{Rgba, Snapshot};
use aurora::foundation::{Key, Point, ResolvedRect, Transform};
use aurora::input::{PointerButton, PointerEvent, PointerPhase};
use aurora::reactive::{Cx, ReactiveRuntime};
use aurora::runtime::Engine;

/// The three boxes of a keyed view, named so they cannot be confused.
///
/// `Engine::rect` is parent-local, `render_rect` is where it is being drawn, and
/// `absolute_rect` is where that lands on screen. Mixing those up is the most common way to
/// aim a pointer at nothing, so a [`Located`] hands back all three under names that say which
/// space each is in, and [`Located::center`] is always in screen space.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Located {
    /// Box in the parent's coordinate space: where layout wants this view.
    pub flow: ResolvedRect,
    /// Box in the parent's space as currently drawn: equal to `flow` unless a render-box
    /// spring is in flight.
    pub render: ResolvedRect,
    /// Box in absolute window coordinates: the only one a pointer can be aimed at.
    pub absolute: ResolvedRect,
}

impl Located {
    /// A point inside the view in absolute window coordinates.
    ///
    /// Averages the on-screen box, so it is always inside whatever the pointer can reach,
    /// even while the box is mid-flight.
    #[inline]
    pub fn center(&self) -> Point {
        Point::new(
            self.absolute.x + self.absolute.width * 0.5,
            self.absolute.y + self.absolute.height * 0.5,
        )
    }

    /// A point at a fraction across and down the on-screen box.
    #[inline]
    pub fn point(&self, fx: f32, fy: f32) -> Point {
        Point::new(
            self.absolute.x + self.absolute.width * fx,
            self.absolute.y + self.absolute.height * fy,
        )
    }

    /// Whether the render box still differs from the flow box.
    #[inline]
    pub fn is_moving(&self) -> bool {
        self.render != self.flow
    }
}

/// Owns an engine plus the runtime and context its page was built against.
pub struct Harness {
    engine: Engine,
    raster: HeadlessRaster,
    /// Frames advanced since the harness was built, excluding the first.
    frame: u64,
    paused: bool,
    dt: f32,
    background: aurora::foundation::Color,
    /// Where a held drag stopped, so it can be released where it stopped.
    last_drag_end: Option<Point>,
}

impl Harness {
    /// Builds a harness over a page component, standing up the hook scope the way
    /// `App::run` does before calling it.
    pub fn page(page: fn() -> aurora::dsl::Element, width: u32, height: u32) -> Harness {
        let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
        let cx = Cx::new(Rc::clone(&runtime));
        Harness::from_runtime(Rc::clone(&runtime), cx.enter(page), width, height)
    }

    /// Builds a harness over an already-built tree.
    pub fn root(root: impl aurora::dsl::IntoElement, width: u32, height: u32) -> Harness {
        let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
        Harness::from_runtime(Rc::clone(&runtime), root, width, height)
    }

    /// Builds a harness over a runtime the page's signals already belong to.
    ///
    /// Needed when a test drives a signal itself rather than clicking, in which case the
    /// signals must be the ones the tree is bound to.
    pub fn from_runtime(
        runtime: Rc<RefCell<ReactiveRuntime>>,
        root: impl aurora::dsl::IntoElement,
        width: u32,
        height: u32,
    ) -> Harness {
        let mut harness = Harness {
            engine: Engine::with_runtime(runtime, root.into_element(), width, height),
            raster: HeadlessRaster::new(),
            frame: 0,
            paused: false,
            dt: 1.0 / 60.0,
            background: aurora::foundation::Color::WHITE,
            last_drag_end: None,
        };
        harness.load_test_font();
        // A freshly mounted tree has no layout yet, so every rect reads zero until a frame
        // runs. Handing back an unrendered harness made every later assertion quietly wrong.
        harness.step();
        harness
    }

    /// Loads the bundled test font, so text pages render instead of panicking.
    pub fn load_test_font(&mut self) {
        self.engine
            .load_font(include_bytes!("../../../aurora/assets/inter-var.ttf"));
    }

    /// Sets the page colour captures are composited against.
    ///
    /// Mirrors what an app configures, so a screenshot shows the page rather than
    /// transparency wherever nothing was painted.
    pub fn with_background(mut self, color: aurora::foundation::Color) -> Self {
        self.engine.set_background(color);
        self.background = color;
        self
    }

    /// The page colour captures are composited against.
    pub fn background(&self) -> aurora::foundation::Color {
        self.background
    }

    /// The current frame as the app would show it: opaque, over the page colour.
    pub fn view(&mut self) -> Snapshot {
        let bg = self.background;
        self.snapshot().over(Rgba {
            r: (bg.r * 255.0).round() as u8,
            g: (bg.g * 255.0).round() as u8,
            b: (bg.b * 255.0).round() as u8,
            a: 255,
        })
    }

    /// Writes the current frame as the app would show it.
    pub fn screenshot(&mut self, path: &str) {
        let frame = self.view();
        if let Err(err) = frame.pixmap().save_png(path) {
            panic!("could not write {path}: {err}");
        }
    }

    /// The delta each [`Harness::step`] advances by.
    pub fn with_delta(mut self, dt: f32) -> Self {
        self.dt = dt;
        self
    }

    /// The engine, for anything this harness does not wrap.
    pub fn engine(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Compares this frame's retained canvas against a full repaint, split by damage
    /// coverage. See [`crate::testing::damage`] for why the split matters.
    pub fn damage_parity(&mut self) -> crate::testing::damage::DamageParity {
        let engine = &mut self.engine;
        let raster = &mut self.raster;
        crate::testing::damage::parity(engine, raster)
    }

    /// Rasterizes the current scene's damage into the retained canvas.
    fn paint(&mut self) -> &Pixmap {
        let size = self.engine.logical_size();
        let text_ctx = self.engine.text_context();
        let damage = self.engine.current_damage().clone();
        self.raster.draw(HeadlessFrame {
            scene: self.engine.scene(),
            damage: &damage,
            size,
            text_ctx: &text_ctx,
        })
    }

    /// The reactive runtime the page's signals live on.
    pub fn runtime(&self) -> Rc<RefCell<ReactiveRuntime>> {
        self.engine.runtime()
    }

    /// Frames advanced so far, excluding the first.
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Advances one frame at the harness delta and rasterizes it.
    ///
    /// While paused this still runs the frame — layout, damage, compile and raster all
    /// happen — but with a zero delta, so the image is current while nothing moves.
    pub fn step(&mut self) -> u64 {
        // Rasterizes, and that is not optional: `frame` advances the damage ring, so a frame that
        // is not painted here has its damage consumed and can never be painted later. Skipping
        // it is not a lazy paint -- it loses those pixels outright, which is why `view()` on a
        // harness that had been stepped without rasterizing reads back as an empty canvas.
        let dt = if self.paused { 0.0 } else { self.dt };
        self.engine.advance(dt);
        self.engine.frame();
        self.paint();
        self.frame += 1;
        self.frame
    }

    /// Advances `n` frames.
    pub fn steps(&mut self, n: u32) -> u64 {
        for _ in 0..n {
            self.step();
        }
        self.frame
    }

    /// Advances until the given frame index, inclusive of the one already taken.
    pub fn step_to(&mut self, frame: u64) -> u64 {
        while self.frame < frame {
            self.step();
        }
        self.frame
    }

    /// Freezes time. Frames still render, but nothing integrates.
    pub fn pause(&mut self) {
        self.paused = true;
        self.engine.set_time_scale(0.0);
    }

    /// Resumes at normal speed.
    pub fn resume(&mut self) {
        self.paused = false;
        self.engine.set_time_scale(1.0);
    }

    /// Advances animation by exactly one quantum, leaving the clock's rate alone.
    ///
    /// The way to walk a frozen transition frame by frame. `pause` followed by `step` does not
    /// do this: the clock is stopped, so an ordinary frame integrates nothing and the frozen
    /// frame is the only one you ever see.
    pub fn step_once(&mut self) {
        self.engine.step_animation();
        self.step();
    }

    /// Runs at a fraction of normal speed, for inspecting a fast transition.
    pub fn slow_motion(&mut self, scale: f32) {
        self.paused = false;
        self.dt = (1.0 / 60.0) * scale;
    }

    /// Whether time is frozen.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Advances until no render box is moving and no ticker is active, or `limit` is hit.
    ///
    /// Returns whether it actually settled, so a test can assert on it rather than silently
    /// asserting against a still-moving scene.
    pub fn settle(&mut self, limit: u32) -> bool {
        // Always advance at least once: a scene can be at rest without ever having been
        // laid out, and returning early there would leave every rect at zero.
        self.step();
        for _ in 0..limit {
            if !self.engine.has_active_motion() {
                return true;
            }
            self.step();
        }
        !self.engine.has_active_motion()
    }

    /// Whether the engine has settled.
    pub fn is_settled(&self) -> bool {
        !self.engine.has_active_motion()
    }

    /// The current frame, copied once so pixel probes do not re-rasterize.
    pub fn snapshot(&mut self) -> Snapshot {
        Snapshot::capture(self.paint())
    }

    /// The current canvas, for saving.
    pub fn canvas(&mut self) -> &Pixmap {
        self.paint()
    }

    /// Where a keyed view is, in all three coordinate spaces at once.
    pub fn at(&self, key: &str) -> Option<Located> {
        let k: Key = key.into();
        Some(Located {
            flow: self.engine.rect(k.clone())?,
            render: self.engine.render_rect(k.clone())?,
            absolute: self.engine.absolute_rect(k)?,
        })
    }

    /// Where a keyed view is, panicking if it does not exist.
    ///
    /// A missing key almost always means the page stopped declaring it, which is a failure
    /// worth reporting as such rather than as a `None` further down.
    pub fn located(&self, key: &str) -> Located {
        self.at(key)
            .unwrap_or_else(|| panic!("no keyed view {key:?} in the tree"))
    }

    /// A keyed view's drawn transform.
    pub fn transform(&self, key: &str) -> Transform {
        self.engine
            .transform(key)
            .unwrap_or_else(|| panic!("no keyed view {key:?} in the tree"))
    }

    /// A keyed view's drawn box in absolute window coordinates.
    pub fn absolute(&self, key: &str) -> ResolvedRect {
        self.located(key).absolute
    }

    /// Queues a raw pointer sample at an absolute point.
    pub fn pointer(&mut self, at: Point, phase: PointerPhase) {
        self.engine.queue_pointer_event(PointerEvent {
            position: at,
            button: PointerButton::Primary,
            phase,
            click_count: 0,
            shift: false,
            alt: false,
            ctrl: false,
            meta: false,
        });
    }

    /// Presses and releases over a keyed view: a click.
    pub fn click(&mut self, key: &str) {
        let at = self.located(key).center();
        self.pointer(at, PointerPhase::Down);
        self.step();
        self.pointer(at, PointerPhase::Up);
        self.step();
    }

    /// Moves the pointer onto a keyed view without pressing, firing hover.
    pub fn hover(&mut self, key: &str) {
        let at = self.located(key).center();
        self.pointer(at, PointerPhase::Move);
        self.step();
    }

    /// Presses over a keyed view and leaves the button down.
    pub fn press(&mut self, key: &str) {
        let at = self.located(key).center();
        self.pointer(at, PointerPhase::Down);
        self.step();
    }

    /// Releases over a keyed view.
    pub fn release(&mut self, key: &str) {
        let at = self.located(key).center();
        self.pointer(at, PointerPhase::Up);
        self.step();
    }

    /// Drags from a keyed view by a screen-space delta, then releases.
    ///
    /// The movement is delivered in `steps` increments so the gesture is genuinely held
    /// throughout, which is what lets drag recognition, capture and per-step reordering all
    /// behave the way they would under a real pointer.
    pub fn drag_by(&mut self, key: &str, dx: f32, dy: f32, steps: u32) {
        self.drag_holding(key, dx, dy, steps);
        let at = self.last_drag_end();
        self.pointer(at, PointerPhase::Up);
        self.step();
    }

    /// Drags by a delta but **leaves the button down**, so the caller can inspect the held
    /// state before releasing.
    ///
    /// This is not a convenience. A drag's most interesting frames are the ones where the
    /// pointer is still down: a lifted row's offset, a scroll view's overscroll, a card's
    /// squeeze. `drag_by` releases before returning, by which point a spring has usually
    /// carried the view most of the way home and the held reading is gone. The end position
    /// is recorded so [`Harness::release_at`] can lift exactly where the drag stopped.
    pub fn drag_holding(&mut self, key: &str, dx: f32, dy: f32, steps: u32) {
        let start = self.located(key).center();
        self.press(key);
        let steps = steps.max(1);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            self.pointer(
                Point::new(start.x + dx * t, start.y + dy * t),
                PointerPhase::Drag,
            );
            self.step();
        }
        self.last_drag_end = Some(Point::new(start.x + dx, start.y + dy));
    }

    /// Where the last held drag stopped, or the pointer's last position if none is held.
    pub fn last_drag_end(&self) -> Point {
        self.last_drag_end.unwrap_or_else(|| Point::new(0.0, 0.0))
    }

    /// Lifts the button where the last held drag stopped.
    ///
    /// Pair with [`Harness::drag_holding`]; using this after a completed `drag_by` is a
    /// mistake, since that gesture has already been released.
    pub fn release_at(&mut self) {
        let at = self.last_drag_end();
        self.pointer(at, PointerPhase::Up);
        self.step();
        self.last_drag_end = None;
    }

    /// Drags by a delta and releases without letting go of the last frame's velocity.
    ///
    /// Identical to [`Harness::drag_by`]; the name marks intent at the call site, since the
    /// point of a flick is that it is still moving when the button lifts.
    pub fn flick(&mut self, key: &str, dx: f32, dy: f32, steps: u32) {
        self.drag_by(key, dx, dy, steps);
    }

    /// Records one sample per frame of a keyed view's drawn transform.
    ///
    /// Asserting on a single frame can only say where something is; a trace says how it got
    /// there, which is the only way to catch a pop, a stall, or a non-monotonic path.
    pub fn trace(&mut self, key: &str, frames: u32) -> MotionTrace {
        let mut samples = Vec::with_capacity(frames as usize);
        for _ in 0..frames {
            samples.push(self.transform(key));
            self.step();
        }
        samples.push(self.transform(key));
        MotionTrace { samples }
    }
}

/// A keyed view's transform sampled once per frame.
#[derive(Clone, Debug, Default)]
pub struct MotionTrace {
    pub samples: Vec<Transform>,
}

impl MotionTrace {
    /// Translation of each sample.
    pub fn xs(&self) -> Vec<f32> {
        self.samples.iter().map(|t| t.tx).collect()
    }

    /// Vertical translation of each sample.
    pub fn ys(&self) -> Vec<f32> {
        self.samples.iter().map(|t| t.ty).collect()
    }

    /// Uniform scale of each sample.
    pub fn scales(&self) -> Vec<f32> {
        self.samples.iter().map(|t| t.a).collect()
    }

    /// Largest single-frame jump in translation, which is what a pop looks like.
    pub fn worst_jump(&self) -> f32 {
        self.samples
            .windows(2)
            .map(|w| {
                let (dx, dy) = (w[1].tx - w[0].tx, w[1].ty - w[0].ty);
                (dx * dx + dy * dy).sqrt()
            })
            .fold(0.0f32, f32::max)
    }

    /// Total distance travelled.
    pub fn path_length(&self) -> f32 {
        self.samples
            .windows(2)
            .map(|w| {
                let (dx, dy) = (w[1].tx - w[0].tx, w[1].ty - w[0].ty);
                (dx * dx + dy * dy).sqrt()
            })
            .sum()
    }

    /// Furthest and least value of a channel.
    pub fn range(&self, values: &[f32]) -> (f32, f32) {
        values
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)))
    }

    /// Whether the channel never reverses direction, which a spring settling should not do
    /// after it has passed its target once.
    pub fn is_monotonic(values: &[f32]) -> bool {
        let (lo, hi) = (f32::MAX, f32::MIN);
        let _ = (lo, hi);
        let mut sign = 0.0f32;
        for w in values.windows(2) {
            let d = w[1] - w[0];
            if d.abs() < f32::EPSILON {
                continue;
            }
            let s = d.signum();
            if sign != 0.0 && s != sign {
                return false;
            }
            sign = s;
        }
        true
    }
}
