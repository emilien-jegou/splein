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
use super::glide::{offset, GlideTicker};
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
    let flip_sig = use_flip(row_ref.clone(), drag.revision.map(Trigger::from), SETTLE);

    // The pointer offset, derived from the pointer's intent and the row's current box rather
    // than accumulated from the grab.
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
    // The release glide: eases the row from its release offset into its slot after a
    // drop, so letting go lands softly instead of snapping. Starts at identity and is
    // seeded by the gesture on release; a fresh grab cancels it outright.
    let settle = use_signal(Transform::IDENTITY);
    let settling = use_signal(false);
    // Register this row's glide state for grab-time cancellation (see Drag): a stale
    // glide from a previous drag masks this row's FLIP for the whole next drag.
    // Keyed by label (overwritten per rebuild, so exactly one entry per row).
    drag.settlers.borrow_mut().insert(
        label.clone(),
        (settling, settle),
    );
    let flip = computed!({
        let is_held = is_held.clone();
        let settling = settling;
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
        let settling = settling;
        // Stay above while gliding home, for the same reason as while held.
        move || {
            if is_held.get() || settling.get() {
                1
            } else {
                0
            }
        }
    });

    let displayed = offset(offset(flip, follow), settle);

    // The transform the node actually binds: a plain signal mirroring the composed
    // value, refreshed in the post-layout phase like `FollowCell::resolve`. Going
    // through a plain signal keeps node delivery independent of derived
    // evaluation history (whatever the chain above does or skips, the node reads
    // one signal that is rewritten every frame its value changes).
    let displayed_sig = use_signal(Transform::IDENTITY);
    {
        let displayed = displayed.clone();
        let displayed_sig = displayed_sig;
        let mirrored = use_signal(false);
        if !mirrored.get() {
            mirrored.set(true);
            row_ref.runtime().borrow_mut().register_post_layout(move |_dt| {
                displayed_sig.set_if_changed(displayed.get());
            });
        }
    }

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
            on:pointer-down={{
                let drag = drag.clone();
                let label = label.clone();
                let settle = settle;
                let settling = settling;
                move |event: &PointerEvent| {
                    // A fresh gesture obsoletes every release-glide still running:
                    // see Drag::cancel_stale_glides.
                    drag.cancel_stale_glides();
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
                let flip = flip_sig;
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
                            out: settle,
                            settling,
                            flip,
                            from,
                            t: 0.0,
                        }));
                    } else {
                        settling.set(false);
                    }
                }
            }}
            on:pointer-up={{
                move |_: &PointerEvent| {
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
