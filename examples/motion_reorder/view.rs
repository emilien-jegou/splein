// Single responsibility: Showcase dragging a list item into a new position.

use std::rc::Rc;

use super::motion_ui::*;
use aurora::dsl::{use_flip, use_follow, use_ref, ElementRef, Trigger};
use aurora::foundation::ResolvedRect;
use aurora::prelude::*;
use aurora::reactive::Ticker;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Web 04 · Drag to reorder";
pub const HOT_WIDTH: u32 = 760;
pub const HOT_HEIGHT: u32 = 660;
pub const HOT_BG: u32 = PAGE;

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

    view! {
        <Shell
            eyebrow_label="WEB · 04 — DRAG TO REORDER"
            title="Grab a row, drop it anywhere in the list."
            copy="While held, the row follows the pointer as a compositor transform, so layout never runs. Crossing a row swaps the list once, and every other row springs from where it was to where layout now puts it. All of it is measured, so squashing the window mid-drag stays exact."
        >
            <Content>
                <Panel
                    head={<Strip
                        left={<Stat label="DRAG" value="compositor transform" />}
                        right={<Stat label="SWAP" value="measured FLIP" />}
                    />}
                >
                    {list(drag)}
                </Panel>
            </Content>
        </Shell>
    }
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
/// Rows are keyed by label through `<for>`, which is what lets each row's hook
/// state — its FLIP above all — survive the rebuild. Keyed by position instead,
/// a reorder would hand each row the animation of whichever item moved into
/// its index.
fn list(drag: Rc<Drag>) -> impl IntoElement {
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
    let list_ref = drag.list.clone();

    view! {
        <Group
            key="list"
            direction="vertical"
            width="fill"
            height="fill"
            gap={GAP}
            // Bound so the gesture can read the row boxes. Nothing is published into a signal: the
            // gesture reads them when the pointer moves, and a measurement that invalidated
            // subscribers would keep the redraw loop awake over a list that is not moving.
            ref={list_ref}
        >
            <for (i, spec) in items key={.label.clone()}>
                <Row index={i} spec={spec} />
            </for>
        </Group>
    }
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
    let (follow_sig, follow_handle) = use_follow(
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
        move || {
            if is_held.get() || settling.get() {
                1
            } else {
                0
            }
        }
    });

    let displayed = offset(offset(flip, follow), settle.clone());

    view! {
        <Group
            key={format!("row-{label}")}
            ref={row_ref}
            direction="horizontal"
            width="fill"
            height="fill"
            radius={12.0}
            fill={ACCENT}
            alignment="center"
            gap={14.0}
            margin={Margin::x(4.0)}
            transform={displayed.clone()}
            z_index={elevation}
            on:pointer-down={{
                let drag = drag.clone();
                let label = label.clone();
                let settle = settle.clone();
                let settling = settling.clone();
                move |event: &PointerEvent| {
                    settling.set(false);
                    settle.set(Transform::IDENTITY);
                    let centre = drag
                        .slots()
                        .get(drag.slot_of(&label))
                        .map(|r| r.y + r.height * 0.5)
                        .unwrap_or(event.position.y);
                    drag.grab_dy.set(event.position.y - centre);
                    drag.held.set(Some(label.clone()));
                }
            }}
            on:drag-start={{
                let drag = drag.clone();
                move |info: &DragInfo| {
                    drag.want_centre
                        .set(Some(info.position.y - drag.grab_dy.get()));
                }
            }}
            on:drag-step={{
                let drag = drag.clone();
                let label = label.clone();
                move |info: &DragInfo| {
                    let centre = info.position.y - drag.grab_dy.get();
                    drag.want_centre.set(Some(centre));

                    let from = drag.slot_of(&label);
                    if let Some(to) = drag.target_slot(centre) {
                        drag.move_row(from, to);
                    }
                }
            }}
            on:drag-end={{
                let drag = drag.clone();
                let settle = settle.clone();
                let settling = settling.clone();
                let displayed = displayed.clone();
                let flip = flip_sig.clone();
                let follow_handle = follow_handle.clone();
                move |info: &DragInfo| {
                    // Resolve the follow offset NOW, before reading anything.
                    // It only materializes in the post-layout phase: releasing
                    // in the same frame as the last pointer sample (a fast
                    // whip) would otherwise seed the glide below from a stale
                    // zero and teleport the row to its slot. See FollowHandle.
                    follow_handle.resolve_now();
                    let from = displayed.get();
                    settle.set(from);
                    settling.set(true);
                    drag.held.set(None);
                    drag.want_centre.set(None);
                    drag.grab_dy.set_with_velocity(0.0, info.velocity.y);
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
            }}
            on:pointer-up={{
                let drag = drag.clone();
                move |_: &PointerEvent| {
                    drag.held.set(None);
                    drag.want_centre.set(None);
                }
            }}
        >
            <Text size={13.0} weight={700u16} color={Color::white_alpha(0.70)} margin={Margin::left(16.0)}>{index + 1}</Text>
            <Text size={14.0} weight={600u16} color={Color::WHITE}>{label}</Text>
            <Group width="fill" />
            <Text size={12.0} color={Color::white_alpha(0.6)} margin={Margin::right(16.0)}>"drag me"</Text>
        </Group>
    }
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
