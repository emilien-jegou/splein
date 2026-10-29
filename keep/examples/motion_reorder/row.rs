// Single responsibility: One draggable row.
//
// A component (not a plain builder function) because both animations' state
// has to outlive the rebuild a reorder causes. Owns the row's FLIP, pointer
// follow, release glide, elevation, and all five gesture handlers; the
// composed transform is mirrored into a plain signal in the post-layout phase
// (see below) so node delivery never depends on derived evaluation history.

use riode::dsl::{use_flip, use_follow, use_ref, Trigger};
use riode::foundation::Transform;
use riode::prelude::*;

use super::chrome::{hairline_dark, VERCEL_PILL};
use super::drag::RowSpec;
use super::glide::offset;
use super::{PAD, SETTLE};

/// One row: its own FLIP, its own pointer follow, and the two summed into one transform.
#[component]
pub fn Row(index: usize, spec: RowSpec) -> impl IntoElement {
    let drag = spec.drag.clone();
    let label = spec.label.clone();

    // A handle to this row's own box, recalled across rebuilds.
    let row_ref = use_ref();

    // The FLIP: reads this row's First and Last positions, and returns the offset that carries
    // it back, which springs to zero as it plays. Overdamped on purpose: displaced rows only
    // ever glide toward their new slot and never cross through each other, which is what a
    // snappier (underdamped) spring would read as mid-drag.
    //
    // This is the row's ONLY displacement spring: the release glide is folded into it on
    // drop (see the end callback) instead of living in a second spring with its own
    // ticker, gate, and cancel protocol. One spring cannot mask itself, go stale, or
    // need cancelling — grab-mid-glide and swap-during-glide both reduce to retargeting.
    let (flip_sig, flip_handle) = use_flip(row_ref.clone(), drag.revision.map(Trigger::from), SETTLE);

    // The pointer offset, derived from the pointer's intent and the row's current box rather
    // than accumulated from the grab. `want_centre` carries pointer-minus-grab-offset
    // (set in the step callback below), so the held row keeps the grabbed point under
    // the cursor; the engine gesture's raw position cannot express that (only the
    // list measures slots), which is why this stays a list-owned signal.
    let (follow_sig, follow_handle) = use_follow(
        row_ref.clone(),
        drag.want_centre,
        drag.grab_dy,
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
    let is_held = computed!({
        let held = drag.held;
        let label = label.clone();
        move || held.get().as_deref() == Some(label.as_str())
    });
    // Landing flag: true from release until the flip spring rests, so a row
    // gliding home paints above its siblings. Derived live every post-layout
    // from the spring itself — never latched — so no timer or gate can stick
    // it on, which is what used to mask flips and teleport rows.
    let landing = use_signal(false);
    // This row's paint level, taken from the shared high-water mark on every
    // grab: later gestures always outrank earlier ones, so the held row can
    // never slide under a still-landing one. Parked back at 0 with `landing`.
    let z_mine = use_signal(0i32);
    use_effect_post_layout({
        let landing = landing;
        let z_mine = z_mine;
        let flip_handle = flip_handle.clone();
        move |_| {
            if landing.get() && !flip_handle.is_running() {
                landing.set(false);
                z_mine.set(0);
            }
        }
    });
    // No gate on the flip: it is always shown. Gating it shut while held hid
    // the live offset the instant the row was grabbed (a snap by the full
    // in-flight displacement) — and the freeze below is what makes showing it
    // safe: a frozen spring is a constant, and a constant cannot fight follow
    // or lag behind the cursor. Only a *live* second dynamic would double-drive,
    // and none exists anymore.
    // Only the held row follows the pointer. `want_centre` is global intent, so an
    // ungated follow would pull every row toward the cursor and stack the list under it.
    let follow = computed!({
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
    let elevation = computed!({
        let is_held = is_held.clone();
        let landing = landing;
        let z_mine = z_mine;
        // Stay above while gliding home, for the same reason as while held.
        // `landing` mirrors the live spring (cleared the frame it rests), so
        // unlike the old settle flag it cannot stick and mask anything.
        //
        // Progressive tiers, not a shared top level: `z_mine` was taken from
        // the high-water mark at grab time, so the latest gesture always
        // outranks earlier ones still landing — no element ever slides behind.
        move || {
            if is_held.get() || landing.get() {
                z_mine.get()
            } else {
                0
            }
        }
    });

    let displayed = offset(flip_sig, follow);

    // The transform the node actually binds: a plain signal mirroring the composed
    // value, refreshed in the post-layout phase like `FollowCell::resolve`. Going
    // through a plain signal keeps node delivery independent of derived
    // evaluation history (whatever the chain above does or skips, the node reads
    // one signal that is rewritten every frame its value changes).
    //
    // `use_effect_post_layout` owns the registration: it re-registers on rebuild with the
    // previous callback torn down first, and unregisters when the row leaves the list —
    // instead of the old guard-signal hack, which registered once and never cleaned up.
    let displayed_sig = use_signal(Transform::IDENTITY);
    use_effect_post_layout({
        let displayed = displayed.clone();
        let displayed_sig = displayed_sig;
        move |_dt| {
            displayed_sig.set_if_changed(displayed.get());
        }
    });

    view! {
        // Outer row: the hit box, the measured slot, and the animated node. Transparent
        // with vertical padding, so the spacing between rows belongs to the rows
        // themselves and presses starting there still grab a row. Content keeps its
        // exact height; slot centres drift by at most a few pixels, which the
        // gesture absorbs by measuring instead of assuming a pitch.
        <Group
            key={format!("row-{label}")}
            ref={row_ref}
            direction="vertical"
            width="fill"
            height="fill"
            padding={Padding::symmetric(PAD, 0.0)}
            transform={displayed_sig}
            z_index={elevation}
            // One gesture instead of five handlers: press/track/release fan out
            // from `Drag`, which also guarantees the node is drag-gated and the
            // gesture state clears on release. The callbacks keep exactly the
            // list-specific work (glide cancellation, grab offset, held
            // identity, reorder swaps, glide seeding); the row's own
            // `on:pointer-up` stays for the one thing the gesture cannot know —
            // clearing list-held state after a press that never became a drag
            // (no drag-end fires then, so nothing else would clear it).
            on:drag={{
                let drag = drag.clone();
                let label = label.clone();
                let follow_handle = follow_handle;
                let displayed = displayed;
                let landing = landing;
                let flip_handle = flip_handle.clone();
                Drag::start(drag.gesture_at)
                    .grab({
                        let drag = drag.clone();
                        let label = label.clone();
                        let flip_handle = flip_handle.clone();
                        move |event: &PointerEvent| {
                            // Freeze the flip FIRST: the row must stop at the
                            // exact mid-flight position the pointer found, so
                            // the grab math below measures against a frozen
                            // spring rather than a decaying one (which would
                            // drift tracking for the whole drag). Unfreezes on
                            // every release path (reseed on drag-end, unpause
                            // on tap-release).
                            flip_handle.set_paused(true);
                            // Take the next paint level: this gesture outranks
                            // everything still landing from earlier ones.
                            drag.z_top.update(|z| *z += 1);
                            z_mine.set(drag.z_top.get());
                            // Grab offset against the layout slot centre. Exact
                            // here precisely because the flip above is frozen:
                            // with zero live offset, layout and visual coincide.
                            let centre = drag
                                .slots()
                                .get(drag.slot_of(&label))
                                .map(|r| r.y + r.height * 0.5)
                                .unwrap_or(event.position.y);
                            drag.grab_dy.set(event.position.y - centre);
                            drag.held.set(Some(label.clone()));
                        }
                    })
                    .step({
                        let drag = drag.clone();
                        let label = label.clone();
                        move |info: &DragInfo| {
                            // Runs on drag-start too (as the first step): same-slot
                            // moves no-op inside `move_row`, so sharing is safe.
                            // Fold the grab offset here, in the only place that
                            // knows it: the row keeps its grabbed point under
                            // the cursor instead of snapping centre-to-cursor.
                            let centre = info.position.y - drag.grab_dy.get();
                            drag.want_centre.set(Some(centre));

                            let from = drag.slot_of(&label);
                            if let Some(to) = drag.target_slot(centre) {
                                drag.move_row(from, to);
                            }
                        }
                    })
                    .end({
                        let drag = drag.clone();
                        let flip_handle = flip_handle.clone();
                        // `info` carries release velocity, deliberately unread: the
                        // glide inherits the flip spring's live velocity instead
                        // (whatever re-inverts left behind), matching the old
                        // settle behavior whose handoff never had a consumer.
                        move |_info: &DragInfo| {
                            // Resolve the follow offset NOW, before reading anything.
                            // It only materializes in the post-layout phase: releasing
                            // in the same frame as the last pointer sample (a fast
                            // whip) would otherwise seed the glide below from a stale
                            // zero and teleport the row to its slot. See FollowHandle.
                            follow_handle.resolve_now();
                            // Fold the release offset into the flip spring itself and
                            // show it: position and velocity carry forward, so the
                            // row glides home from exactly where the pointer left
                            // it. No second spring, no ticker, no gate, nothing to
                            // cancel or go stale — grab-mid-glide and swap-during-
                            // glide both reduce to retargeting.
                            flip_handle.reseed(displayed.get());
                            landing.set(true);
                            drag.held.set(None);
                            drag.want_centre.set(None);
                            drag.grab_dy.set(0.0);
                        }
                    })
            }}
            on:pointer-up={{
                let drag = drag.clone();
                let flip_handle = flip_handle;
                move |_: &PointerEvent| {
                    // Tap-release (no drag-end fires): resume the frozen glide
                    // so the row keeps landing instead of sticking mid-flight.
                    flip_handle.set_paused(false);
                    drag.held.set(None);
                    drag.want_centre.set(None);
                }
            }}
        >
            <Group
                direction="horizontal"
                width="fill"
                height="fill"
                radius={14.0}
                fill={VERCEL_PILL}
                stroke={Stroke::inside(1.0, hairline_dark())}
                alignment="center"
                gap={24.0}
                margin={Margin::x(2.0)}
            >
                <Text size={13.0} weight={700u16} color={Color::white_alpha(0.70)} margin={Margin::left(16.0)}>{index + 1}</Text>
                <Text size={14.0} weight={600u16} color={Color::WHITE}>{label}</Text>
                <Group width="fill" />
                <Text size={12.0} color={Color::white_alpha(0.6)} margin={Margin::right(16.0)}>"drag me"</Text>
            </Group>
        </Group>
    }
}
