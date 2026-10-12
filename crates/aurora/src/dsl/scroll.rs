// Single responsibility: Client-side scroll viewport — clip box + absolute content + clamped offset.
//
// A `ScrollView` is three existing primitives composed: a fixed-size `clip(true)`
// viewport (a layout boundary, so scrolling never re-lays-out siblings), one
// absolute content child carrying the offset on the `SCROLL` channel (paint-class,
// so ticks never lay out either), and a post-layout clamp (`max = content −
// viewport`) that writes the offset signal in the same frame. No raster primitive
// was added: backends already replay clipped chunks, and the overscroll band
// (§overscroll) keeps entering rows encoded.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use crate::dsl::component::IntoElement;
use crate::dsl::element::Element;
use crate::dsl::element_ref::ElementRef;
use crate::dsl::flip::use_ref;
use crate::dsl::group::group;
use crate::foundation::{Anchor, Direction, Margin, Radius, Size};
use crate::input::{ClickInfo, DragInfo, HoverInfo, KeyCode, KeyEvent, PointerEvent, Propagation, WheelInfo};
use crate::prelude::component;
use crate::reactive::hooks::{current_scope, next_slot, no_scope, recall_value, remember_value};
use crate::reactive::{use_signal, Readable, Signal, Ticker};

/// Default preload band in logical px per edge: steady scrolling replays
/// retained chunks instead of re-encoding entering rows (~16 MB at 4K).
pub const DEFAULT_OVERSCROLL: f32 = 512.0;
/// Keyboard line step in logical px.
pub const SCROLL_LINE: f32 = 40.0;
/// Fling decay rate (1/s): velocity halves roughly every 100 ms.
const FLING_DECAY: f32 = 7.0;
/// Fling stops below this velocity (px/s).
const FLING_STOP: f32 = 24.0;
/// Incoming fling velocity clamp (px/s): release spikes must not teleport.
const FLING_MAX: f32 = 4000.0;
/// Scrollbar thumb minimum length (px) and width (px).
const THUMB_MIN: f32 = 24.0;
const THUMB_WIDTH: f32 = 8.0;
/// Thumb inset from the viewport's top-right corner (px).
const THUMB_INSET: f32 = 4.0;
/// Top/bottom lane clearance (px): keeps the pill out of rounded-corner
/// clip zones so its ends never slice against the card curve.
const THUMB_EDGE: f32 = 6.0;
/// Thumb opacity while active; fades to transparent after `FADE_DELAY`.
const THUMB_ALPHA: f32 = 0.5;
/// Idle seconds before the thumb starts fading.
const FADE_DELAY: f32 = 1.0;
/// Fade-in duration in seconds (appearing thumb ramps, never pops).
const FADE_IN: f32 = 0.2;
/// Fade duration in seconds.
const FADE_TIME: f32 = 0.3;
/// Right-edge zone (px) where a click pages instead of passing through.
const TRACK_ZONE: f32 = 24.0;
/// Rubber-band shaping: visual overshoot for a raw overshoot, in px.
/// `over / (1 + |over| / BAND)` — ~half at one band, asymptoting to the band.
const RUBBER_BAND: f32 = 120.0;
/// Spring stiffness and damping for the edge return (1/s, 1/s).
const SPRING_K: f32 = 180.0;
const SPRING_C: f32 = 22.0;
/// Snap thresholds: rest position error (px) and velocity (px/s).
const SPRING_X_EPS: f32 = 0.5;
const SPRING_V_EPS: f32 = 30.0;

/// Maps a raw offset through edge resistance: inside the range it passes
/// through untouched; past an end it yields with Apple-style falloff.
fn rubber_band(raw: f32, min: f32, max: f32) -> f32 {
    let clamped = raw.clamp(min, max);
    let over = raw - clamped;
    if over == 0.0 {
        clamped
    } else {
        clamped + over / (1.0 + over.abs() / RUBBER_BAND)
    }
}

/// Scrollbar policy for a `ScrollView`.
///
/// `Auto` (default) shows an overlay thumb whenever the content overflows;
/// `Visible` pins it on; `Hidden` removes it. `Auto` and `Visible` currently
/// behave identically (fade-after-idle is a follow-up, not a behavior gap:
/// nothing hides information today).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Scrollbar {
    #[default]
    Auto,
    Visible,
    Hidden,
}

/// Programmatic handle to a scroll viewport's offset state.
///
/// Owns nothing in the tree: both signals live in hook memory, so a handle
/// created by [`use_scroll`] in a component body survives rebuilds, and one
/// passed to `ScrollView` drives (and is driven by) the viewport.
#[derive(Clone)]
pub struct ScrollHandle {
    /// Current offset in logical px, always clamped post-layout.
    pub offset: Signal<f32>,
    /// Scrollable range (`content − viewport`, floored at zero), published
    /// post-layout. Read it to size thumbs and gate "scroll to end".
    pub max: Signal<f32>,
}

impl ScrollHandle {
    /// Jumps the viewport; the post-layout clamp pulls out-of-range values
    /// back in the same frame, so callers never clamp by hand.
    pub fn scroll_to(&self, y: f32) {
        self.offset.set(y);
    }

    /// Relative jump; same clamping contract as [`Self::scroll_to`].
    pub fn scroll_by(&self, dy: f32) {
        self.offset.set(self.offset.get() + dy);
    }

    /// Jumps so uniform row `index` of height `row_h` lands at the viewport
    /// top (clamped like any jump). The virtual-list fast path to deep links.
    pub fn scroll_to_index(&self, index: usize, row_h: f32) {
        self.offset.set(index as f32 * row_h.max(1.0));
    }

    /// Least-scroll reveal of a content-space band `[rect_y, rect_y+rect_h)`
    /// in a viewport of height `view_h`: moves just enough to bring it fully
    /// into view, or nothing when already visible. `rect_y` is measured from
    /// the content origin (window-space minus viewport origin plus the
    /// current offset); `max` bounds the result like any jump.
    pub fn scroll_into_view(&self, rect_y: f32, rect_h: f32, view_h: f32) {
        let cur = self.offset.get();
        let next = if rect_y < cur {
            rect_y
        } else if rect_y + rect_h > cur + view_h {
            rect_y + rect_h - view_h
        } else {
            return;
        };
        self.offset.set(next.clamp(0.0, self.max.get().max(0.0)));
    }
}

/// Allocates a scroll handle in the ambient component scope.
pub fn use_scroll(initial: f32) -> ScrollHandle {
    ScrollHandle {
        offset: use_signal(initial),
        max: use_signal(0.0),
    }
}

/// Scroll observer callback. Fires post-layout when the clamped offset
/// changes — never during motion, so it must not drive motion (no feedback).
pub struct OnScroll(pub Rc<dyn Fn(f32)>);

impl<F: Fn(f32) + 'static> From<F> for OnScroll {
    fn from(f: F) -> Self {
        Self(Rc::new(f))
    }
}

/// Row factory for a [`VirtualScrollView`]: index in `[0, len)` to element.
///
/// `row={|i| ...}` in `view!` works through the blanket `From` — any
/// `Fn(usize) -> Element` closure converts.
#[derive(Clone)]
pub struct RowFn(pub Rc<dyn Fn(usize) -> Element>);

impl<F: Fn(usize) -> Element + 'static> From<F> for RowFn {
    fn from(f: F) -> Self {
        Self(Rc::new(f))
    }
}

/// Per-gesture drag state, owned by one `ScrollView` instance and shared by
/// its drag closures (all `Fn`, so mutation goes through cells).
#[derive(Clone)]
struct ScrollGesture {
    /// Offset when the press began; steps position absolutely from it, so
    /// per-sample rounding never accumulates into drift.
    start: Cell<f32>,
    /// Axis lock: once a gesture proves horizontal it stays ignored until
    /// release (a diagonal that starts vertical keeps scrolling).
    horizontal: Cell<bool>,
}

/// Frame pump for scroll motion: fling, edge spring, and thumb fade all
/// need frames after their triggering event is gone, and a quiet app runs
/// none. One pump per viewport, attached at mount, retained for life;
/// `is_active` gates actual frame requests, so a settled viewport costs one
/// predicate check per frame and nothing else.
///
/// The weak reference is load-bearing: a hot-reload rebuild files a fresh
/// cell while the old pump still lives — upgrading a dead cell retires it
/// instead of integrating a dead tree's motion forever.
struct ScrollPump {
    cell: Weak<ScrollClamp>,
}

impl ScrollPump {
    /// Whether any scroll motion wants frames right now. Reads plain cells
    /// only (see `last_max`): signal reads would borrow the runtime mutably
    /// and panic under the shared borrow this is called with.
    fn live(cell: &ScrollClamp) -> bool {
        if cell.fling.get() != 0.0 {
            return true;
        }
        let (off, max) = (cell.last_emitted.get(), cell.last_max.get());
        if off < 0.0 || off > max {
            return true;
        }
        if !(cell.thumbs && max > 0.0) {
            return false;
        }
        // Fading either way, or within the idle window before the fade-out.
        let op = cell.last_op.get();
        (op > 0.0 && op < THUMB_ALPHA) || cell.idle.get() < FADE_DELAY + FADE_TIME
    }
}

impl Ticker for ScrollPump {
    fn advance(&mut self, _dt: f32) -> bool {
        // Retained unconditionally: liveness is answered by `is_active`, and
        // re-attaching on every motion start would stack duplicates across
        // rebuilds. Integration stays in the post-layout clamp (same frame).
        self.cell.upgrade().is_some()
    }

    fn is_active(&self) -> bool {
        self.cell.upgrade().is_some_and(|c| Self::live(&c))
    }
}

/// Post-layout scroll cell: owned by one `ScrollView` instance, filed in hook
/// memory so rebuilds find (rather than duplicate) it.
struct ScrollClamp {
    offset: Signal<f32>,
    max: Signal<f32>,
    /// Live fling velocity (offset px/s), seeded by drag release, eaten by
    /// wheel/drag takeover and viewport ends. Doubles as the edge-spring
    /// velocity when the offset rests past an end.
    fling: Rc<Cell<f32>>,
    /// Whether a drag gesture currently holds the content. While held, the
    /// clamp publishes `max` but never corrects the offset — the gesture
    /// owns it (including deliberate overshoot); physics resumes on release.
    held: Rc<Cell<bool>>,
    /// Rubber-band past the ends (Apple-style, on by default). Off means the
    /// hard clamp: overshoot is pulled in same-frame, flings die at the edge.
    bounce: bool,
    viewport: ElementRef,
    content: ElementRef,
    /// Thumb geometry outputs (identity when the scrollbar is hidden).
    thumb_y: Signal<f32>,
    thumb_h: Signal<f32>,
    thumb_travel: Signal<f32>,
    /// Thumb opacity output: full while active, fading after `FADE_DELAY`
    /// idle. Drives both paint and presence (a faded thumb unmounts, so it
    /// never intercepts clicks meant for content).
    thumb_op: Signal<f32>,
    /// Frame-granularity mirror of `max`, for the pump predicate below.
    /// Ticker predicates (`is_active`) run under a shared runtime borrow,
    /// so they must never touch signals (whose reads take a mutable borrow
    /// for tracking). Plain cells keep them total.
    last_max: Cell<f32>,
    /// Frame-granularity mirror of the thumb opacity, for the pump predicate
    /// (transitions either way keep frames coming) and the fade-in ramp.
    last_op: Cell<f32>,
    /// Seconds since the thumb started appearing (fade-in ramp clock).
    reveal: Cell<f32>,
    /// Seconds since the offset last changed (frame dt accumulated here, so
    /// frozen time never fades).
    idle: Cell<f32>,
    /// Whether the pointer is over the viewport. Forces the thumb visible
    /// while hovered (without touching `idle`, so leaving resumes the fade
    /// where stillness left it). Set by enter/move/leave handlers.
    hovered: Rc<Cell<bool>>,
    thumbs: bool,
    on_scroll: Option<OnScroll>,
    last_emitted: Cell<f32>,
    /// Whether the first resolve adopted the offset silently. Mount starts
    /// faded (idle `INFINITY`): without priming, the `NaN` initial compares
    /// unequal to everything and the first frame would flash the thumb.
    primed: Cell<bool>,    /// Whether the offset rested past an end last frame. Distinguishes a
    /// fresh programmatic overshoot (a jump: snap it like the hard clamp)
    /// from live gesture overshoot (held or flung: spring it back).
    was_beyond: Cell<bool>,
    /// Armed by gesture writers (wheel, thumb, keys) alongside a rubber
    /// write: this frame's overshoot — if any — springs rather than snaps.
    /// Direct signal writes never arm it, so jumps stay same-frame. Consumed
    /// every resolve.
    soft: Rc<Cell<bool>>,
}

impl ScrollClamp {
    /// Free motion with bounce: inside the range the fling decays; past an
    /// end the offset strains against a spring and returns on release (or on
    /// its own when flung past). Semi-implicit Euler is stable here
    /// (`k·dt² << 1` at frame rates) and exact per `dt`, so frozen stepping
    /// replays it deterministically.
    fn resolve_free(&self, dt: f32, max: f32) {
        let cur = self.offset.get();
        let beyond = cur < 0.0 || cur > max;
        // Fresh programmatic overshoot (a jump, not a gesture): snap it
        // exactly like the hard clamp, on every frame including frozen
        // ones — assignment, not integration. Gesture writers (wheel, thumb
        // drags, keys) arm `soft` alongside their rubber write, and held or
        // flung overshoot never reaches this branch, so it only ever sees
        // direct signal writes.
        let soft = self.soft.take();
        if beyond && !soft && !self.was_beyond.get() && self.fling.get() == 0.0 {
            self.offset.set(cur.clamp(0.0, max));
        } else if dt > 0.0 {
            if cur < 0.0 || cur > max {
                let target = cur.clamp(0.0, max);
                let mut x = cur - target;
                let mut v = self.fling.get();
                v += (-SPRING_K * x - SPRING_C * v) * dt;
                x += v * dt;
                if x.abs() < SPRING_X_EPS && v.abs() < SPRING_V_EPS {
                    self.offset.set(target);
                    self.fling.set(0.0);
                } else {
                    self.offset.set(target + x);
                    self.fling.set(v);
                }
            } else {
                let v = self.fling.get();
                if v != 0.0 {
                    self.offset.set(cur + v * dt);
                    let decayed = v * (-dt * FLING_DECAY).exp();
                    self.fling.set(if decayed.abs() < FLING_STOP {
                        0.0
                    } else {
                        decayed
                    });
                }
            }
        }
        let settled = self.offset.get();
        self.was_beyond.set(settled < 0.0 || settled > max);
    }

    /// Integrates free motion, then recomputes `max`. While held, the gesture
    /// owns the offset (correction skipped, `max` still published). Runs in
    /// the post-layout phase, so the boxes it reads are this frame's, and the
    /// corrected signal is picked up by this frame's paint drain. `dt` is the
    /// frame's ticked time (zero while frozen), so F8/`.`-stepping motion
    /// advances it deterministically. Clamped to 50ms: after a stall the
    /// physics must ease back in, not teleport (which also fast-forwards
    /// fades and ramps past their curves in one frame).
    fn resolve(&self, dt: f32) {
        let dt = dt.min(1.0 / 20.0);
        let (Some(view), Some(content)) = (self.viewport.rect(), self.content.rect()) else {
            return;
        };
        // Heights are translation-invariant, so the in-flight offset cannot
        // skew the range it is clamped against.
        let max = (content.height - view.height).max(0.0);
        self.max.set_if_changed(max);
        self.last_max.set(max);
        if !self.held.get() {
            if self.bounce {
                self.resolve_free(dt, max);
            } else {
                // Hard clamp: integrate the fling, then pull into range and
                // eat it at the edge.
                let v = self.fling.get();
                if v != 0.0 && dt > 0.0 {
                    self.offset.set(self.offset.get() + v * dt);
                    let decayed = v * (-dt * FLING_DECAY).exp();
                    self.fling.set(if decayed.abs() < FLING_STOP {
                        0.0
                    } else {
                        decayed
                    });
                }
                let clamped = self.offset.get().clamp(0.0, max);
                if clamped != self.offset.get() {
                    self.offset.set(clamped);
                    self.fling.set(0.0);
                }
            }
        }
        let current = self.offset.get();
        if !self.primed.get() {
            self.primed.set(true);
            self.last_emitted.set(current);
        } else if current != self.last_emitted.get() {
            self.last_emitted.set(current);
            self.idle.set(0.0);
            if let Some(cb) = &self.on_scroll {
                (cb.0)(current);
            }
        } else {
            self.idle.set(self.idle.get() + dt);
        }
        if self.thumbs {
            // The thumb travels a lane inset from both edges (see
            // THUMB_EDGE): ends park clear of rounded-corner clips instead
            // of slicing against them.
            let lane_h = (view.height - 2.0 * THUMB_EDGE).max(0.0);
            let thumb_h = if max <= 0.0 {
                0.0
            } else {
                (view.height * view.height / content.height)
                    .clamp(THUMB_MIN, view.height)
                    .min(lane_h)
            };
            self.thumb_h.set_if_changed(thumb_h);
            let travel = (lane_h - thumb_h).max(0.0);
            self.thumb_travel.set_if_changed(travel);
            let y = if max <= 0.0 {
                0.0
            } else {
                // Clamped for display: an overshooting offset parks the thumb
                // at the edge it strains against.
                THUMB_EDGE + current.clamp(0.0, max) / max * travel
            };
            self.thumb_y.set_if_changed(y);
            // Fade-after-idle: activity (any offset write) resets above.
            // Hover forces full visibility without touching the idle clock,
            // so leaving resumes the fade where stillness left it.
            let target = if max <= 0.0 {
                0.0
            } else if self.hovered.get() {
                THUMB_ALPHA
            } else {
                let idle = self.idle.get();
                if idle < FADE_DELAY {
                    THUMB_ALPHA
                } else if idle >= FADE_DELAY + FADE_TIME {
                    0.0
                } else {
                    THUMB_ALPHA * (1.0 - (idle - FADE_DELAY) / FADE_TIME)
                }
            };
            // Fade-in ramp: appearing from fully hidden ramps over FADE_IN
            // instead of popping (the reported flicker was the instant jump
            // compounded by hover flapping between thumb and content).
            // Reappearing mid-fade jumps straight back; hiding snaps only
            // from a ramp that never started.
            let op = if target > 0.0 && self.last_op.get() == 0.0 {
                self.reveal.set(self.reveal.get() + dt);
                (THUMB_ALPHA * self.reveal.get() / FADE_IN).min(target)
            } else {
                if target == 0.0 {
                    self.reveal.set(0.0);
                }
                target
            };
            self.thumb_op.set_if_changed(op);
            self.last_op.set(op);
        }
    }
}

/// A vertical scroll viewport.
///
/// Uncontrolled by default — zero wiring, internal offset dropped with the
/// component (hot-reload rebuilds reset it, like any unnamed hook state):
///
/// ```ignore
/// view! {
///     <ScrollView width={400.0} height={600.0}>
///         {rows}
///     </ScrollView>
/// }
/// ```
///
/// Controlled / programmatic via a handle from [`use_scroll`]:
///
/// ```ignore
/// let sc = use_scroll(0.0);
/// view! {
///     <ScrollView width={400.0} height={600.0} handle={sc.clone()}>
///         {rows}
///     </ScrollView>
/// }
/// # sc.scroll_to(0.0);
/// ```
///
/// `offset={signal}` binds a caller-owned `Signal<f32>` directly instead of a
/// handle. `sticky={element}` pins one overlay in viewport space.
/// `scrollbar` shows an overlay thumb while the content overflows. Only
/// `Direction::Vertical` is honored in v1; anything else warns and behaves
/// vertically.
///
/// Interaction is contain-only: drags (axis-locked vertical), wheel, fling,
/// and keys stop at the innermost viewport and never chain outward. A drag
/// that starts on a clickable-but-undraggable row still scrolls (gesture
/// transfer); a press on selectable text selects (selection wins the press).
#[component]
pub fn ScrollView(
    width: Size,
    height: Size,
    direction: Option<Direction>,
    overscroll: Option<f32>,
    offset: Option<Signal<f32>>,
    handle: Option<ScrollHandle>,
    on_scroll: Option<OnScroll>,
    sticky: Option<Element>,
    scrollbar: Option<Scrollbar>,
    bounce: Option<bool>,
    children: Vec<Element>,
) -> impl IntoElement {
    let direction = direction.unwrap_or(Direction::Vertical);
    let overscroll = overscroll.unwrap_or(DEFAULT_OVERSCROLL);
    let scrollbar = scrollbar.unwrap_or(Scrollbar::Auto);
    let bounce = bounce.unwrap_or(true);
    if direction != Direction::Vertical {
        tracing::warn!(
            "aurora: ScrollView only scrolls vertically in v1; ignoring {direction:?}"
        );
    }
    // Unconditional hooks: call order is identity, so every signal exists on
    // every build whether the caller brought their own or not.
    let owned = use_scroll(0.0);
    let offset_sig = offset
        .or_else(|| handle.clone().map(|h| h.offset))
        .unwrap_or_else(|| owned.offset.clone());
    let max_sig = handle
        .map(|h| h.max)
        .unwrap_or_else(|| owned.max.clone());
    let viewport_ref = use_ref();
    let content_ref = use_ref();
    let thumb_y = use_signal(0.0f32);
    let thumb_h = use_signal(0.0f32);
    let thumb_travel = use_signal(0.0f32);
    let thumb_op = use_signal(0.0f32);

    // Instance memory below the ordinal hooks: fling velocity and gesture
    // state survive rebuilds like any hook state (a hot-reload mid-fling
    // keeps gliding instead of stopping dead).
    let Some(scope) = current_scope() else {
        no_scope()
    };
    let fling: Rc<Cell<f32>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<f32>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(0.0f32));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let gesture: Rc<ScrollGesture> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<ScrollGesture>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(ScrollGesture {
                    start: Cell::new(0.0),
                    horizontal: Cell::new(false),
                });
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    // Drag-hold flag: a rebuild mid-gesture must not drop it (the clamp
    // would snap a rubber-banded offset back while the finger is down).
    // The soft flag rides alongside: gesture rubber writes arm the spring
    // path for this frame (see `resolve_free`).
    let held: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let soft: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let hovered: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };

    // One clamp registration per cell: a rebuilt body recalls the filed cell
    // and skips re-registering; a fresh cell (any scope — benches, harnesses,
    // and direct mounts have no stable parent) always registers. Resolve is
    // idempotent, so even a pathological double registration stays correct.
    let slot = next_slot(&scope);
    match scope
        .is_stable()
        .then(|| recall_value::<Rc<ScrollClamp>>(&scope, slot))
        .flatten()
    {
        Some(_) => {}
        None => {
            let cell = Rc::new(ScrollClamp {
                offset: offset_sig.clone(),
                max: max_sig.clone(),
                fling: Rc::clone(&fling),
                held: Rc::clone(&held),
                bounce,
                viewport: viewport_ref.clone(),
                content: content_ref.clone(),
                thumb_y: thumb_y.clone(),
                thumb_h: thumb_h.clone(),
                thumb_travel: thumb_travel.clone(),
                thumb_op: thumb_op.clone(),
                idle: Cell::new(f32::INFINITY),
                thumbs: scrollbar != Scrollbar::Hidden,
                on_scroll,
                last_emitted: Cell::new(f32::NAN),
                last_max: Cell::new(0.0),
                last_op: Cell::new(0.0),
                reveal: Cell::new(0.0),
                primed: Cell::new(false),
                was_beyond: Cell::new(false),
                soft: Rc::clone(&soft),
                hovered: Rc::clone(&hovered),
            });
            if scope.is_stable() {
                remember_value(&scope, slot, &cell);
            }
            offset_sig.attach(Box::new(ScrollPump {
                cell: Rc::downgrade(&cell),
            }));
            scope
                .runtime()
                .borrow_mut()
                .register_post_layout(move |dt| cell.resolve(dt));
        }
    }

    let viewport = scroll_viewport_group(
        width,
        height,
        overscroll,
        viewport_ref.clone(),
        &offset_sig,
        &max_sig,
        &fling,
        &held,
        &gesture,
        &viewport_ref,
        &thumb_y,
        &thumb_h,
        bounce,
        &soft,
        &hovered,
    )
    .children([
        group()
            .absolute()
            .isolate_children(true)
            .scroll_offset(offset_sig.clone())
            .direction(Direction::Vertical)
            // Percent, not Fill: absolute Fill resolves to zero desired width
            // (`resolve_desired_dimension`), while Percent(1.0) tracks the
            // viewport exactly.
            .width(Size::Percent(1.0))
            .bind_ref(content_ref)
            .children(children)
            .into_element(),
    ]);
    let viewport = match sticky {
        Some(pinned) => viewport
            .children([group().absolute().children([pinned]).into_element()]),
        None => viewport,
    };
    if scrollbar == Scrollbar::Hidden {
        return viewport.into_element();
    }
    viewport
        .children([scroll_thumb(
            &offset_sig,
            &max_sig,
            &fling,
            &held,
            &thumb_y,
            &thumb_h,
            &thumb_travel,
            &thumb_op,
            bounce,
            &soft,
            &hovered,
        )])
        .into_element()
}

/// The viewport box: clip + scrollable marker + all gesture handlers.
///
/// Drags axis-lock on the first step (vertical scrolls, horizontal locks out
/// for the gesture); steps position absolutely from the press offset so
/// per-sample rounding never drifts. Wheel and keys write the (published)
/// max-clamped offset directly; release velocity becomes a fling integrated
/// post-layout. Every handler stops the walk (contain).
#[allow(clippy::too_many_arguments)]
fn scroll_viewport_group(
    width: Size,
    height: Size,
    overscroll: f32,
    viewport_ref: ElementRef,
    offset_sig: &Signal<f32>,
    max_sig: &Signal<f32>,
    fling: &Rc<Cell<f32>>,
    held: &Rc<Cell<bool>>,
    gesture: &Rc<ScrollGesture>,
    key_view_ref: &ElementRef,
    thumb_y_sig: &Signal<f32>,
    thumb_h_sig: &Signal<f32>,
    bounce: bool,
    soft: &Rc<Cell<bool>>,
    hovered: &Rc<Cell<bool>>,
) -> crate::dsl::group::GroupDef {
    let offset = offset_sig.clone();
    let fling_start = Rc::clone(fling);
    let held_start = Rc::clone(held);
    let gesture_start = Rc::clone(gesture);
    let offset_step = offset_sig.clone();
    let max_step = max_sig.clone();
    let gesture_step = Rc::clone(gesture);
    let fling_end = Rc::clone(fling);
    let held_end = Rc::clone(held);
    let gesture_end = Rc::clone(gesture);
    let offset_wheel = offset_sig.clone();
    let max_wheel = max_sig.clone();
    let fling_wheel = Rc::clone(fling);
    let soft_wheel = Rc::clone(soft);
    let offset_key = offset_sig.clone();
    let max_key = max_sig.clone();
    let fling_key = Rc::clone(fling);
    let key_view = key_view_ref.clone();
    let hover_enter = Rc::clone(hovered);
    let hover_move = Rc::clone(hovered);
    let hover_leave = Rc::clone(hovered);

    group()
        .width(width)
        .height(height)
        .clip(true)
        .scrollable(true)
        .overscroll(overscroll)
        .bind_ref(viewport_ref)
        // Hover keeps the thumb visible while the pointer rests over the
        // viewport (no signal writes: showing needs no invalidation beyond
        // this frame, and the pump sustains the fade after leaving).
        .on_pointer_enter(move |_: &HoverInfo| {
            hover_enter.set(true);
        })
        .on_pointer_move(move |_: &PointerEvent| {
            hover_move.set(true);
        })
        .on_pointer_leave(move |_: &HoverInfo| {
            hover_leave.set(false);
        })
        .on_drag_start_handled(move |_info: &DragInfo| {
            gesture_start.start.set(offset.get());
            gesture_start.horizontal.set(false);
            // Grabbing live content takes over from any fling.
            fling_start.set(0.0);
            held_start.set(true);
            Propagation::Stop
        })
        .on_drag_step_handled(move |info: &DragInfo| {
            if gesture_step.horizontal.get() {
                return Propagation::Stop;
            }
            let total = info.delta;
            if total.x.abs() > total.y.abs() {
                gesture_step.horizontal.set(true);
                return Propagation::Stop;
            }
            let max = max_step.get();
            let raw = gesture_step.start.get() - total.y;
            offset_step.set(if bounce {
                rubber_band(raw, 0.0, max)
            } else {
                raw.clamp(0.0, max)
            });
            Propagation::Stop
        })
        .on_drag_end_handled(move |info: &DragInfo| {
            let horizontal = gesture_end.horizontal.get();
            gesture_end.horizontal.set(false);
            held_end.set(false);
            if !horizontal {
                fling_end.set((-info.velocity.y).clamp(-FLING_MAX, FLING_MAX));
            }
            Propagation::Stop
        })
        .on_wheel_handled(move |wheel: &WheelInfo| {
            fling_wheel.set(0.0);
            let max = max_wheel.get();
            if max <= 0.0 {
                // Contain: consume even when there is nowhere to go, so an
                // outer scroller never picks up the remainder.
                return Propagation::Stop;
            }
            // Positive delta_y is scroll-up intent (wheel up, swipe up):
            // offset decreases, like a drag that moved the content down.
            // Rubber-bands past the ends like a drag; discrete jumps (keys,
            // track clicks) stay hard-clamped.
            let raw = offset_wheel.get() - wheel.delta_y;
            soft_wheel.set(true);
            offset_wheel.set(if bounce {
                rubber_band(raw, 0.0, max)
            } else {
                raw.clamp(0.0, max)
            });
            Propagation::Stop
        })
        .on_key_handled(move |key: &KeyEvent| {
            fling_key.set(0.0);
            let max = max_key.get().max(0.0);
            let current = offset_key.get();
            let page = (key_view
                .rect()
                .map(|r| r.height)
                .unwrap_or(600.0)
                - 48.0)
                .max(64.0);
            let next = match key.code {
                KeyCode::Up => current - SCROLL_LINE,
                KeyCode::Down => current + SCROLL_LINE,
                KeyCode::PageUp => current - page,
                KeyCode::PageDown => current + page,
                KeyCode::Home => 0.0,
                KeyCode::End => max,
                KeyCode::Char(' ') => {
                    if key.shift {
                        current - page
                    } else {
                        current + page
                    }
                }
                _ => return Propagation::Bubble,
            };
            // Discrete jumps stay hard-clamped (keys included: holding an
            // arrow at the edge must sit still, not judder through
            // rubber-band cycles). Continuous gestures (drag, wheel, thumb)
            // rubber-band above.
            offset_key.set(next.clamp(0.0, max));
            Propagation::Stop
        })
        .on_click({
            let offset = offset_sig.clone();
            let max = max_sig.clone();
            let fling = Rc::clone(fling);
            let view = key_view_ref.clone();
            let thumb_y = thumb_y_sig.clone();
            let thumb_h = thumb_h_sig.clone();
            move |info: &ClickInfo| {
                // Track paging: a press in the right-edge zone (that the
                // thumb did not capture) jumps a page toward the pressed
                // side. Content clicks elsewhere fall through untouched.
                fling.set(0.0);
                let max = max.get();
                if max <= 0.0 {
                    return;
                }
                let Some(v) = view.rect() else { return };
                if info.position.x < v.x + v.width - TRACK_ZONE {
                    return;
                }
                let page = (v.height - 48.0).max(64.0);
                let center = v.y + thumb_y.get() + thumb_h.get() * 0.5;
                let next = if info.position.y < center {
                    offset.get() - page
                } else {
                    offset.get() + page
                };
                offset.set(next.clamp(0.0, max));
            }
        })
}

/// Overlay scrollbar thumb: absolute top-right, sized and travelled by the
/// post-layout signals, faded by idle time. Dragging it jumps with momentum
/// parity on release; it captures its own press, so the viewport gesture
/// never starts from the thumb, and swallows its own clicks so they never
/// page the track beneath it.
fn scroll_thumb(
    offset_sig: &Signal<f32>,
    max_sig: &Signal<f32>,
    fling: &Rc<Cell<f32>>,
    held: &Rc<Cell<bool>>,
    thumb_y: &Signal<f32>,
    thumb_h: &Signal<f32>,
    thumb_travel: &Signal<f32>,
    thumb_op: &Signal<f32>,
    bounce: bool,
    soft: &Rc<Cell<bool>>,
    hovered: &Rc<Cell<bool>>,
) -> Element {
    let fling = Rc::clone(fling);
    let offset = offset_sig.clone();
    let grab = Rc::new(Cell::new(0.0f32));
    let grab_step = Rc::clone(&grab);
    let offset_step = offset_sig.clone();
    let max_step = max_sig.clone();
    let soft_step = Rc::clone(soft);
    let travel_step = thumb_travel.clone();
    let thumb_h_step = thumb_h.clone();
    let fling_end = Rc::clone(&fling);
    let held_start = Rc::clone(held);
    let held_end = Rc::clone(held);
    let max_end = max_sig.clone();
    let travel_end = thumb_travel.clone();
    let max_visible = max_sig.clone();
    let op_visible = thumb_op.clone();
    let hover_thumb_enter = Rc::clone(hovered);
    let hover_thumb_move = Rc::clone(hovered);

    group()
        .absolute()
        .anchor(Anchor::TopRight)
        .width(THUMB_WIDTH)
        .height(thumb_h.clone())
        .x(-THUMB_INSET)
        .y(thumb_y.clone())
        .radius(Radius::max())
        .fill(0x0A0A0Au32)
        .opacity(thumb_op.clone())
        // Gone when faded: an invisible thumb must not intercept clicks meant
        // for content (or silently swallow thumb drags). Presence toggles
        // layout once per fade cycle — bounded and rare.
        .visible(move || max_visible.get() > 0.0 && op_visible.get() > 0.0)
        .on_click_handled(move |_| Propagation::Stop)
        // The thumb is viewport chrome: hovering it counts as hovering the
        // viewport, so moving between thumb and content never flaps the
        // fade. (Without this, the thumb appearing under the cursor fires a
        // viewport leave, which starts fading the thumb just shown.)
        .on_pointer_enter(move |_: &HoverInfo| {
            hover_thumb_enter.set(true);
        })
        .on_pointer_move(move |_: &PointerEvent| {
            hover_thumb_move.set(true);
        })
        .on_drag_start_handled(move |_info: &DragInfo| {
            grab.set(offset.get());
            fling.set(0.0);
            held_start.set(true);
            Propagation::Stop
        })
        .on_drag_step_handled(move |info: &DragInfo| {
            let (travel, max) = (travel_step.get(), max_step.get());
            if travel > 0.0 && max > 0.0 {
                let raw = grab_step.get() + info.delta.y / travel * max;
                // Overshoot past the ends is capped at half the viewport
                // height: a grabbed thumb must never throw content further
                // than that, however hard the drag. (Content drags keep the
                // rubber asymptote; the spring reconciles both.)
                let view_h = travel + thumb_h_step.get();
                let (lo, hi) = (-0.5 * view_h, max + 0.5 * view_h);
                soft_step.set(true);
                offset_step.set(if bounce {
                    rubber_band(raw, 0.0, max).clamp(lo, hi)
                } else {
                    raw.clamp(0.0, max)
                });
            }
            Propagation::Stop
        })
        .on_drag_end_handled(move |info: &DragInfo| {
            // Momentum parity: the thumb hands its content-space velocity to
            // the same fling the content drag uses. Note the sign: dragging
            // the thumb down increases the offset (scrollbar convention),
            // opposite to a content drag.
            held_end.set(false);
            let (travel, max) = (travel_end.get(), max_end.get());
            if travel > 0.0 && max > 0.0 {
                fling_end.set((info.velocity.y / travel * max).clamp(-FLING_MAX, FLING_MAX));
            }
            Propagation::Stop
        })
        .into_element()
}

/// Tracks the mounted row window `[start, end)` for one `VirtualScrollView`.
///
/// Updated post-layout from the live offset and viewport height, with an
/// overscan band on both sides so the window only changes when scrolling
/// nears its edge — steady ticks inside the band cost no rebuild. The window
/// signal feeds the content builder, which is tracked under the wrapper's
/// measure subscription, so a window change re-lays-out the (bounded) window.
struct WindowTracker {
    offset: Signal<f32>,
    view: ElementRef,
    window: Signal<(usize, usize)>,
    len: usize,
    row_h: f32,
    overscan: usize,
}

/// Window-slide hysteresis, in rows: the window only rebuilds when the
/// needed band comes this close to either edge. Steady ticks inside cost no
/// rebuild; crossing still slides by the full overscan.
const WINDOW_HYSTERESIS: usize = 4;

impl WindowTracker {
    fn resolve(&self) {
        let Some(view) = self.view.rect() else {
            return;
        };
        let off = self.offset.get().max(0.0);
        let need_first = (off / self.row_h).floor() as usize;
        let need_last = ((off + view.height) / self.row_h).ceil() as usize;
        let (cur_s, cur_e) = self.window.get();
        // Keep the old window while the needed band sits comfortably inside
        // it; an empty window (initial mount) always recomputes.
        if cur_e > cur_s
            && need_first >= cur_s.saturating_add(WINDOW_HYSTERESIS).min(self.len)
            && need_last.saturating_add(WINDOW_HYSTERESIS).min(self.len) <= cur_e
        {
            return;
        }
        let start = need_first.saturating_sub(self.overscan).min(self.len);
        let end = (need_last + self.overscan).min(self.len).max(start);
        self.window.set_if_changed((start, end));
    }
}

/// A virtualized vertical scroll viewport: `len` uniform rows of `row_height`,
/// of which only the visible window (+ overscan) is mounted.
///
/// Same viewport, clamp, scrollbar, and contain-interaction as [`ScrollView`];
/// only child production differs. Rows are keyed by index (`vrow-{i}`), so a
/// row that stays in the window across slides keeps its node (and its state);
/// rows leaving the window unmount.
///
/// Uniform-height contract: a row taller than `row_height` overpaints its
/// successor (no per-row measure in v1 — that is the follow-up, with a
/// measure cache). `len == 0` mounts an empty, unscrollable viewport.
///
/// ```ignore
/// view! {
///     <VirtualScrollView width={Size::fill()} height={600.0} len={100_000}
///         row_height={36.0} row={|i| Row(i)}>
///     </VirtualScrollView>
/// }
/// ```
#[component]
pub fn VirtualScrollView(
    width: Size,
    height: Size,
    len: usize,
    row_height: f32,
    row: RowFn,
    direction: Option<Direction>,
    overscroll: Option<f32>,
    offset: Option<Signal<f32>>,
    handle: Option<ScrollHandle>,
    on_scroll: Option<OnScroll>,
    sticky: Option<Element>,
    scrollbar: Option<Scrollbar>,
    bounce: Option<bool>,
) -> impl IntoElement {
    let direction = direction.unwrap_or(Direction::Vertical);
    let overscroll = overscroll.unwrap_or(DEFAULT_OVERSCROLL);
    let scrollbar = scrollbar.unwrap_or(Scrollbar::Auto);
    let bounce = bounce.unwrap_or(true);
    let row_h = row_height.max(1.0);
    if direction != Direction::Vertical {
        tracing::warn!(
            "aurora: VirtualScrollView only scrolls vertically in v1; ignoring {direction:?}"
        );
    }
    let owned = use_scroll(0.0);
    let offset_sig = offset
        .or_else(|| handle.clone().map(|h| h.offset))
        .unwrap_or_else(|| owned.offset.clone());
    let max_sig = handle
        .map(|h| h.max)
        .unwrap_or_else(|| owned.max.clone());
    let viewport_ref = use_ref();
    let content_ref = use_ref();
    let thumb_y = use_signal(0.0f32);
    let thumb_h = use_signal(0.0f32);
    let thumb_travel = use_signal(0.0f32);
    let thumb_op = use_signal(0.0f32);
    let overscan = ((overscroll / row_h).ceil() as usize).max(2);
    // First paint already covers the first screen when the height is fixed;
    // otherwise the post-layout tracker fills the window a frame later.
    let initial_end = match height {
        Size::Fixed(h) => ((h / row_h).ceil() as usize + overscan).min(len),
        _ => (overscan * 2).min(len),
    };
    let window = use_signal((0usize, initial_end));

    let Some(scope) = current_scope() else {
        no_scope()
    };
    let fling: Rc<Cell<f32>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<f32>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(0.0f32));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let gesture: Rc<ScrollGesture> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<ScrollGesture>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(ScrollGesture {
                    start: Cell::new(0.0),
                    horizontal: Cell::new(false),
                });
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    // Drag-hold flag: a rebuild mid-gesture must not drop it (the clamp
    // would snap a rubber-banded offset back while the finger is down).
    // The soft flag rides alongside: gesture rubber writes arm the spring
    // path for this frame (see `resolve_free`).
    let held: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let soft: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let hovered: Rc<Cell<bool>> = {
        let slot = next_slot(&scope);
        scope
            .is_stable()
            .then(|| recall_value::<Rc<Cell<bool>>>(&scope, slot))
            .flatten()
            .unwrap_or_else(|| {
                let fresh = Rc::new(Cell::new(false));
                if scope.is_stable() {
                    remember_value(&scope, slot, &fresh);
                }
                fresh
            })
    };
    let slot = next_slot(&scope);
    match scope
        .is_stable()
        .then(|| recall_value::<Rc<ScrollClamp>>(&scope, slot))
        .flatten()
    {
        Some(_) => {}
        None => {
            let cell = Rc::new(ScrollClamp {
                offset: offset_sig.clone(),
                max: max_sig.clone(),
                fling: Rc::clone(&fling),
                held: Rc::clone(&held),
                bounce,
                viewport: viewport_ref.clone(),
                content: content_ref.clone(),
                thumb_y: thumb_y.clone(),
                thumb_h: thumb_h.clone(),
                thumb_travel: thumb_travel.clone(),
                thumb_op: thumb_op.clone(),
                idle: Cell::new(f32::INFINITY),
                thumbs: scrollbar != Scrollbar::Hidden,
                on_scroll,
                last_emitted: Cell::new(f32::NAN),
                last_max: Cell::new(0.0),
                last_op: Cell::new(0.0),
                reveal: Cell::new(0.0),
                primed: Cell::new(false),
                was_beyond: Cell::new(false),
                soft: Rc::clone(&soft),
                hovered: Rc::clone(&hovered),
            });
            if scope.is_stable() {
                remember_value(&scope, slot, &cell);
            }
            offset_sig.attach(Box::new(ScrollPump {
                cell: Rc::downgrade(&cell),
            }));
            scope
                .runtime()
                .borrow_mut()
                .register_post_layout(move |dt| cell.resolve(dt));
        }
    }
    // Window tracker: same once-per-cell rule as the clamp.
    {
        let slot = next_slot(&scope);
        match scope
            .is_stable()
            .then(|| recall_value::<Rc<WindowTracker>>(&scope, slot))
            .flatten()
        {
            Some(_) => {}
            None => {
                let tracker = Rc::new(WindowTracker {
                    offset: offset_sig.clone(),
                    view: viewport_ref.clone(),
                    window: window.clone(),
                    len,
                    row_h,
                    overscan,
                });
                if scope.is_stable() {
                    remember_value(&scope, slot, &tracker);
                }
                scope
                    .runtime()
                    .borrow_mut()
                    .register_post_layout(move |_dt| tracker.resolve());
            }
        }
    }

    let total_h = len as f32 * row_h;
    let window_sig = window.clone();
    // Rows are absolute at their index pitch: the window is a sparse subset,
    // so in-flow stacking would pile row 990 at the top (its 990 predecessors
    // are not mounted). Absolute placement derives from anchor+margins, which
    // is static-safe — unlike pose `.y()`, whose static form is silently
    // dropped. Rendering row 1000 mounts only the window: unmounted rows cost
    // nothing anywhere (layout, damage, and compile only visit live nodes).
    let viewport = scroll_viewport_group(
        width,
        height,
        overscroll,
        viewport_ref.clone(),
        &offset_sig,
        &max_sig,
        &fling,
        &held,
        &gesture,
        &viewport_ref,
        &thumb_y,
        &thumb_h,
        bounce,
        &soft,
        &hovered,
    )
    .children([
        group()
            .absolute()
            .isolate_children(true)
            .scroll_offset(offset_sig.clone())
            .direction(Direction::Vertical)
            // Percent, not Fill: absolute Fill resolves to zero desired width
            // (`resolve_desired_dimension`), while Percent(1.0) tracks the
            // viewport exactly.
            .width(Size::Percent(1.0))
            .height(total_h)
            .bind_ref(content_ref)
            .children_of(move || {
                let (s, e) = window_sig.get();
                (s..e)
                    .map(|i| {
                        let build = || {
                            group()
                                .key(format!("vrow-{i}"))
                                .absolute()
                                // Percent like the content wrapper: absolute
                                // Fill resolves to zero width.
                                .width(Size::Percent(1.0))
                                .height(row_h)
                                .margin(Margin::top(i as f32 * row_h))
                                .children([(row.0)(i)])
                                .into_element()
                        };
                        match crate::reactive::Cx::current() {
                            Some(cx) => cx.enter_keyed(format!("vrow-{i}"), build),
                            None => build(),
                        }
                    })
                    .collect()
            })
            .into_element(),
    ]);
    let viewport = match sticky {
        Some(pinned) => viewport
            .children([group().absolute().children([pinned]).into_element()]),
        None => viewport,
    };
    if scrollbar == Scrollbar::Hidden {
        return viewport.into_element();
    }
    viewport
        .children([scroll_thumb(
            &offset_sig,
            &max_sig,
            &fling,
            &held,
            &thumb_y,
            &thumb_h,
            &thumb_travel,
            &thumb_op,
            bounce,
            &soft,
            &hovered,
        )])
        .into_element()
}
