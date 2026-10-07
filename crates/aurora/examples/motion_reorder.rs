// Single responsibility: Showcase dragging a list item into a new position.

#[path = "common/mod.rs"]
mod common;
#[path = "common/motion_ui.rs"]
mod motion_ui;

use std::rc::Rc;

use aurora::dsl::{use_flip, use_follow, use_ref, ElementRef, Trigger};
use aurora::foundation::ResolvedRect;
use aurora::prelude::*;
use aurora::reactive::Ticker;
use common::ExampleCli;
use motion_ui::*;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 660;
/// Declared gap between rows.
///
/// The one geometric constant in the demo, and a declaration rather than a measurement on
/// purpose: it is an *input* to layout. What the drag needs is the resulting distance between
/// rows, and that is measured from the rows themselves every step — see [`Drag::target_slot`].
///
/// Nothing declares a row height. The rows fill the list and layout divides the space it was
/// actually given, so there is no height to fall out of step with the window.
pub const GAP: f32 = 10.0;
/// Settle spring for displaced (non-held) rows.
///
/// Local to the example so it can be tuned without touching the shared presets: overdamped,
/// so a row glides toward its new slot instead of overshooting through its neighbour.
const SETTLE: Spring = Spring::new(300.0, 38.0, 1.0);
/// Rows offered for reordering.
pub const LABELS: [&str; 5] = [
    "Baseline",
    "Underline",
    "Strikethrough",
    "Highlight",
    "Squiggle",
];

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Web 04 · Drag to reorder")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(PAGE))
        .run(Page);
}

/// A draggable list that reorders itself from reactive state.
///
/// Three concerns stay separated, which is the point of the demo. The *list* is a signal;
/// reordering it re-reconciles the children by key. The *held row* follows the pointer. The
/// *other rows* travel from where they were to where layout now puts them, and that last part is
/// done here rather than by the engine.
///
/// The FLIP used to be an engine feature, on the claim that only layout knows where a row landed.
/// It does know, and it will say — but only if asked. Each row holds a handle to its own box,
/// [`use_flip`] does the four steps from measured positions, and [`use_follow`] keeps the held
/// row under the pointer from the same geometry. Nothing here computes a pitch, because there is
/// no single pitch to compute: a squashed window leaves the rows unequal.
#[component]
pub fn Page() -> impl IntoElement {
    let order = use_signal(LABELS.map(str::to_string).to_vec());
    // Bumped on every reorder, which is what tells each row's FLIP that the arrangement changed.
    //
    // A revision rather than the order itself: comparing five labels per row per frame is work
    // proportional to the list, and the only question is "is this a different arrangement from
    // the one whose positions I cached".
    let revision = use_signal(0u32);
    // Which row is held, by identity rather than by slot. A reorder changes a row's index, so
    // tracking the index would drop the hold the moment the list shuffled underneath it.
    let held = use_signal(None::<String>);
    // Where in the held row the pointer took hold, so the row keeps that point under the cursor
    // instead of snapping its centre to it.
    let grab_dy = use_signal(0.0f32);
    // Where the pointer is asking the held row's centre to be, in window coordinates.
    let want_centre = use_signal(None::<f32>);

    let drag = Rc::new(Drag {
        order: order.clone(),
        revision: revision.clone(),
        held: held.clone(),
        grab_dy: grab_dy.clone(),
        want_centre: want_centre.clone(),
        // One handle for the list, which is how the gesture learns where the rows are.
        list: use_ref(),
    });

    page(
        "WEB · 04 — DRAG TO REORDER",
        "Grab a row, drop it anywhere in the list.",
        "While held, the row follows the pointer as a compositor transform, so layout never runs. Crossing a row swaps the list once, and every other row springs from where it was to where layout now puts it. All of it is measured, so squashing the window mid-drag stays exact.",
        content(panel(
            strip(
                stat("DRAG", "compositor transform"),
                stat("SWAP", "measured FLIP"),
            ),
            list(drag),
        )),
    )
}

/// The state the gesture needs, shared by every row.
///
/// Carried on the order signal's items rather than passed as list arguments, because
/// `children_labels_keyed` hands a row only `(index, item)` and threading a context through it
/// would mean changing the builder's shape. Keeping the payload on the item means the key is
/// still the label and the shared state rides along with it.
#[derive(Clone)]
struct RowSpec {
    label: String,
    drag: Rc<Drag>,
}

struct Drag {
    order: Signal<Vec<String>>,
    revision: Signal<u32>,
    held: Signal<Option<String>>,
    grab_dy: Signal<f32>,
    want_centre: Signal<Option<f32>>,
    /// Handle on the list, whose children *are* the rows.
    ///
    /// Reading the row boxes through the list rather than through each row's own handle is what
    /// lets one gesture compare the pointer against every slot without any of them cooperating.
    list: ElementRef,
}

impl Drag {
    /// Moves a row and bumps the revision each row's FLIP watches.
    fn move_row(&self, from: usize, to: usize) {
        if from == to {
            return;
        }
        self.order.update(|list| {
            let moved = list.remove(from);
            list.insert(to, moved);
        });
        self.revision.update(|r| *r += 1);
    }

    /// A row's current slot.
    fn slot_of(&self, label: &str) -> usize {
        self.order
            .get()
            .iter()
            .position(|l| l == label)
            .unwrap_or(0)
    }

    /// Every row's box this frame, in slot order.
    fn slots(&self) -> Vec<ResolvedRect> {
        self.list.child_rects().to_vec()
    }

    /// The slot the pointer is asking for, by measuring rather than by arithmetic.
    ///
    /// There is deliberately no pitch in this demo. Rows that fill a list share whatever space
    /// is left over, but each is then clamped at its own min-content floor, so a squashed window
    /// makes the gaps *unequal* — and then no single stride exists to divide a distance by.
    /// Reading the boxes sidesteps the question instead of answering it.
    fn target_slot(&self, centre: f32) -> Option<usize> {
        self.slots()
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = (a.y + a.height * 0.5 - centre).abs();
                let db = (b.y + b.height * 0.5 - centre).abs();
                da.total_cmp(&db)
            })
            .map(|(i, _)| i)
    }
}

/// The list, whose rows are rebuilt whenever the order signal changes.
///
/// Rows are keyed by label through `children_labels_keyed`, which is what lets each row's hook
/// state — its FLIP above all — survive the rebuild. Keyed by position instead, a reorder would
/// hand each row the animation of whichever item moved into its index.
fn list(drag: Rc<Drag>) -> GroupDef {
    let rows = drag.order.clone();
    let items = rows.map({
        let drag = drag.clone();
        move |labels| {
            labels
                .into_iter()
                .map(|label| RowSpec {
                    label,
                    drag: Rc::clone(&drag),
                })
                .collect::<Vec<_>>()
        }
    });

    group()
        .key("list")
        .direction(Direction::Vertical)
        .width(Size::fill())
        .height(Size::fill())
        .gap(GAP)
        // Bound so the gesture can read the row boxes. Nothing is published into a signal: the
        // gesture reads them when the pointer moves, and a measurement that invalidated
        // subscribers would keep the redraw loop awake over a list that is not moving.
        .bind_ref(drag.list.clone())
        .children_labels_keyed(items, |spec: &RowSpec| spec.label.clone(), Row)
}

/// One row: its own FLIP, its own pointer follow, and the two summed into one transform.
///
/// A component rather than a plain builder function, because both animations' state has to
/// outlive the rebuild a reorder causes.
#[component]
fn Row(index: usize, spec: RowSpec) -> impl IntoElement {
    let drag = spec.drag.clone();
    let label = spec.label.clone();

    // A handle to this row's own box, recalled across rebuilds.
    let row_ref = use_ref();

    // The FLIP: reads this row's First and Last positions, and returns the offset that carries
    // it back, which springs to zero as it plays. Overdamped on purpose: displaced rows only
    // ever glide toward their new slot and never cross through each other, which is what a
    // snappier (underdamped) spring would read as mid-drag.
    let flip_sig = use_flip(row_ref.clone(), drag.revision.map(Trigger::from), SETTLE);

    // The pointer offset, derived from the pointer's intent and the row's current box rather
    // than accumulated from the grab.
    let follow_sig = use_follow(
        row_ref.clone(),
        drag.want_centre.clone(),
        drag.grab_dy.clone(),
    );

    // A held row is already being told where to go — by the pointer — so it must not also carry
    // a FLIP offset toward the slot the reorder moved it into. Two commands on one axis make the
    // row lag a stride behind the cursor and then catch up.
    //
    // Gating on the hold rather than on "the offset is non-zero" is deliberate: every row binds
    // both offsets unconditionally, so keying off a non-zero value would disable the FLIP
    // everywhere.
    //
    // Plain closures with automatic dependency tracking: each `derived!` subscribes to
    // every signal its body reads — no `.map()`/`.combine()` chains.
    let is_held = derived!({
        let held = drag.held.clone();
        let label = label.clone();
        move || held.get().as_deref() == Some(label.as_str())
    });
    // The release glide: eases the row from its release offset into its slot after a
    // drop, so letting go lands softly instead of snapping. Starts at identity and is
    // seeded by the gesture on release; a fresh grab cancels it outright.
    let settle = use_signal(Transform::IDENTITY);
    let settling = use_signal(false);
    let flip = derived!({
        let is_held = is_held.clone();
        let settling = settling.clone();
        move || {
            // Kept shut while settling for the same reason as while held: the raw FLIP
            // still holds decaying offsets from the drag's reorders, and letting them in
            // would fight the glide.
            if is_held.get() || settling.get() {
                Transform::IDENTITY
            } else {
                flip_sig.get()
            }
        }
    });
    // Only the held row follows the pointer. `want_centre` is global intent, so an
    // ungated follow would pull every row toward the cursor and stack the list under it.
    let follow = derived!({
        let is_held = is_held.clone();
        move || {
            if is_held.get() {
                follow_sig.get()
            } else {
                Transform::IDENTITY
            }
        }
    });
    // The held row paints above the rows it passes. A plain sibling paints in document
    // order, so without this the rows gliding into their new slots slide *over* the row
    // being dragged. Bound like the offsets above: `held` flips on press and release, the
    // node repaints, layout never runs.
    let elevation = derived!({
        let is_held = is_held.clone();
        let settling = settling.clone();
        // Stay above while gliding home, for the same reason as while held.
        move || if is_held.get() || settling.get() { 1 } else { 0 }
    });

    let displayed = offset(offset(flip, follow), settle.clone());
    let release = Release {
        settle,
        settling,
        displayed: displayed.clone(),
        flip: flip_sig.clone(),
    };

    row(index, &label)
        .bind_ref(row_ref)
        .transform(displayed)
        .z_index(elevation)
        .pipe_gesture(drag, label, release)
        .into_element()
}

/// What the gesture needs to land a row softly: the glide offset it seeds on release,
/// whether a glide is in flight, the row's total transform to capture at release, and
/// the raw FLIP output the glide must wait for before handing control back.
struct Release {
    settle: Signal<Transform>,
    settling: Signal<bool>,
    displayed: Derived<Transform>,
    flip: Signal<Transform>,
}

/// Seconds a released row takes to ease into its slot.
const GLIDE_SECS: f32 = 0.22;

/// Eases a release offset down to zero on an ease-out cubic, then retires.
///
/// Self-retiring is the point: unlike the follow ticker, which lives as long as the row,
/// this only asks for frames while a glide is actually in flight, so a list at rest costs
/// nothing. A fresh grab disarms it by clearing `settling` (see `gesture`), and the next
/// advance then retires without touching the offset the new hold just claimed.
///
/// The glide ends when the timer is done *and* the raw FLIP is at rest — whichever comes
/// later. Ending on the timer alone reopens the FLIP gate while the spring is still
/// decaying from the drag's last reorder, and the residual snaps in as a visible jitter
/// exactly when the glide looks finished. The FLIP snaps to exactly zero on settle, so an
/// exact comparison is the right handoff test.
struct GlideTicker {
    out: Signal<Transform>,
    settling: Signal<bool>,
    flip: Signal<Transform>,
    from: Transform,
    t: f32,
}

impl Ticker for GlideTicker {
    fn advance(&mut self, dt: f32) -> bool {
        // Cancelled by a fresh grab, or already finished: retire without writing, so the
        // offset the new hold owns is never clobbered a frame late.
        if !self.settling.get() {
            return false;
        }
        self.t += dt;
        let k = (self.t / GLIDE_SECS).min(1.0);
        let ease = 1.0 - (1.0 - k).powi(3);
        let rest = 1.0 - ease;
        self.out.set(Transform::from_translation(
            self.from.tx * rest,
            self.from.ty * rest,
        ));
        if k >= 1.0 && self.flip.get() == Transform::IDENTITY {
            self.settling.set(false);
            return false;
        }
        true
    }
}

/// Sums two vertical offsets into the one transform a row is drawn with.
///
/// Both animations write a y translation and neither writes an x, so this is a sum rather than a
/// matrix product. Said out loud because a general compose would be wrong here in a way that
/// only shows up as a diagonal drift.
fn offset(
    flip: impl SignalRead<Transform> + 'static,
    follow: impl SignalRead<Transform> + 'static,
) -> Derived<Transform> {
    derived!(move || {
        let a = flip.get();
        let b = follow.get();
        Transform::from_translation(a.tx + b.tx, a.ty + b.ty)
    })
}

/// Wires one row's whole gesture: claim it, follow it, reorder as it crosses a row, and hand
/// the release velocity back.
///
/// The hand-off matters here more than anywhere else: without it the row stops dead the instant
/// the pointer lifts, however hard it was thrown.
#[allow(clippy::too_many_arguments)]
fn gesture(row: GroupDef, drag: Rc<Drag>, label: String, release: Release) -> GroupDef {
    // Recorded on the press, not on the drag's start. Sub-threshold moves produce no
    // drag events at all (a 1px click wiggle is a click, not a drag), so a grab recorded
    // at drag start would not exist yet if the first step arrived any other way — and
    // the press is the one moment the pointer is guaranteed to be on the row.
    row.on_pointer_down({
        let drag = drag.clone();
        let label = label.clone();
        let settle = release.settle.clone();
        let settling = release.settling.clone();
        move |event: &PointerEvent| {
            // A fresh grab cancels any glide still in flight and reclaims the offset, so the
            // row answers the pointer immediately instead of finishing a landing nobody wants.
            // Clearing `settling` disarms the ticker; it retires on its next advance.
            settling.set(false);
            settle.set(Transform::IDENTITY);
            // Measured against the box layout gave this row, read through the list rather than
            // from the builder's index: a reorder since the last build makes that stale.
            let centre = drag
                .slots()
                .get(drag.slot_of(&label))
                .map(|r| r.y + r.height * 0.5)
                .unwrap_or(event.position.y);
            drag.grab_dy.set(event.position.y - centre);
            drag.held.set(Some(label.clone()));
        }
    })
    // Seeded on the drag's start as well as on every step. The sample that crosses the
    // threshold reports the start but no step of its own, so without this the row sits
    // still through the first few pixels of a gesture and then jumps to the pointer once
    // the first step lands, which reads as the row lagging rather than as a drag that
    // just began.
    .on_drag_start({
        let drag = drag.clone();
        move |info: &DragInfo| {
            drag.want_centre
                .set(Some(info.position.y - drag.grab_dy.get()));
        }
    })
    .on_drag_step({
        let drag = drag.clone();
        let label = label.clone();
        move |info: &DragInfo| {
            // The centre the pointer is asking for. Every other decision is measured against it.
            let centre = info.position.y - drag.grab_dy.get();
            drag.want_centre.set(Some(centre));

            let from = drag.slot_of(&label);
            if let Some(to) = drag.target_slot(centre) {
                drag.move_row(from, to);
            }
        }
    })
    .on_drag_end({
        let drag = drag.clone();
        let settle = release.settle.clone();
        let settling = release.settling.clone();
        let displayed = release.displayed.clone();
        let flip = release.flip.clone();
        move |info| {
            // Captured before the hold clears, while the gates still show the row where the
            // pointer left it. Seeding the glide from this (rather than from zero) is what
            // turns the snap into a landing: the first eased frame continues the last held
            // one instead of jumping home.
            let from = displayed.get();
            settle.set(from);
            settling.set(true);
            drag.held.set(None);
            // Cleared so the follow offset reads exactly zero, which is what releases the row.
            drag.want_centre.set(None);
            // The throw's own velocity, so a flick is thrown rather than stopped dead.
            drag.grab_dy.set_with_velocity(0.0, info.velocity.y);
            // Skipped only when there is truly nothing to wait for: a sub-pixel offset *and*
            // a settled FLIP. Otherwise the ticker eases home and holds the FLIP gate shut
            // until the spring is at rest too, so control hands over with no residual jump.
            if (from.ty.abs() >= 1.0 || from.tx.abs() >= 1.0)
                || flip.get() != Transform::IDENTITY
            {
                settle.register_ticker(Box::new(GlideTicker {
                    out: settle.clone(),
                    settling: settling.clone(),
                    flip: flip.clone(),
                    from,
                    t: 0.0,
                }));
            } else {
                settling.set(false);
            }
        }
    })
    // A press released below the drag threshold never becomes a drag, so `on_drag_end`
    // never runs for it — without this the hold (and the elevation) would stick on a
    // plain click. Exclusive with the path above: a release that ended a drag reports
    // `DragEnded`, never `Released`, so the two cannot double-clear.
    .on_pointer_up(move |_| {
        drag.held.set(None);
        drag.want_centre.set(None);
    })
}

/// Wires a row's gesture onto it.
trait PipeGesture {
    fn pipe_gesture(self, drag: Rc<Drag>, label: String, release: Release) -> GroupDef;
}

impl PipeGesture for GroupDef {
    fn pipe_gesture(self, drag: Rc<Drag>, label: String, release: Release) -> GroupDef {
        gesture(self, drag, label, release)
    }
}

/// A row's presented surface.
fn row(index: usize, label: &str) -> GroupDef {
    group()
        // Keyed by label, not by index: a reorder then reads as two rows changing places rather
        // than as every row being replaced.
        .key(format!("row-{label}"))
        .direction(Direction::Horizontal)
        .width(Size::fill())
        .height(Size::fill())
        .radius(12.0)
        .fill(Color::hex(ACCENT))
        .alignment(Alignment::Center)
        .gap(14.0)
        .margin(Margin::x(4.0))
        .children((
            text(format!("{}", index + 1))
                .size(13.0)
                .weight(700)
                .color(Color::white_alpha(0.70))
                .margin(Margin::left(16.0)),
            text(label.to_string())
                .size(14.0)
                .weight(600)
                .color(Color::WHITE),
            group().width(Size::fill()),
            text("drag me")
                .size(12.0)
                .color(Color::white_alpha(0.6))
                .margin(Margin::right(16.0)),
        ))
}
