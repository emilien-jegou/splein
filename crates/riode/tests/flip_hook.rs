// Single responsibility: A FLIP animates from measured geometry, and reads do not disturb layout.

use std::cell::RefCell;
use std::rc::Rc;

use riode::dsl::{use_flip, use_ref, ElementRef, Trigger};
use riode::foundation::{Color, Direction, Size};
use riode::prelude::*;
use riode::reactive::{Cx, ReactiveRuntime, Signal};
use riode_tiny_skia::testing::Harness;

/// A row that starts hard left and moves to the far side of its container when `moved` flips.
///
/// The FLIP is the point of this fixture: the element is laid out somewhere new and it has to
/// *arrive* rather than teleport. That makes it the smallest thing that fails if the hook measures
/// the wrong thing, applies its answer a frame late, or never springs at all.
///
/// Laid out with a fixed width beside a fill spacer, so the move is a real change of position
/// rather than a change of size — a size change would move the centre without the element going
/// anywhere, which a FLIP has nothing to say about.
#[component]
fn Slider(moved: Signal<bool>, refs: Rc<RefCell<Vec<ElementRef>>>) -> Element {
    let handle = use_ref();
    refs.borrow_mut().push(handle.clone());

    let flip = use_flip(
        handle.clone(),
        moved.map(|m| Trigger::from(m as u64)),
        Spring::snappy(),
    );

    // The children are built from the signal, so flipping it genuinely *reorders* them and the
    // box is laid out somewhere new. A FLIP needs layout to have moved: a fixture that only
    // toggled a flag would test the trigger, not the animation, and would pass either way.
    group()
        .key("slider")
        .width(Size::fill())
        .height(Size::fill())
        .direction(Direction::Horizontal)
        .children_of({
            let moved = moved.clone();
            let handle = handle.clone();
            move || {
                let box_el = || {
                    group()
                        .key("box")
                        .width(120.0)
                        .height(Size::fill())
                        .fill(Color::hex(0x2563EB))
                        .bind_ref(handle.clone())
                        .transform(flip.clone())
                        .into_element()
                };
                let spacer = || {
                    group()
                        .width(Size::fill())
                        .height(Size::fill())
                        .into_element()
                };
                if moved.get() {
                    vec![spacer(), box_el()]
                } else {
                    vec![box_el(), spacer()]
                }
            }
        })
        .into_element()
}

/// Mounts the fixture, returning the harness, the signal, and the box's own handle.
fn mount() -> (Harness, Signal<bool>, ElementRef) {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let moved = Signal::new(Rc::clone(&rt), false);
    let refs = Rc::new(RefCell::new(Vec::new()));
    let for_tree = moved.clone();
    let tree = Cx::new(Rc::clone(&rt)).enter({
        let refs = Rc::clone(&refs);
        move || Slider(for_tree.clone(), refs)
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), tree, 400, 200);
    assert!(h.settle(600), "the scene must settle before it is moved");

    let handle = refs.borrow()[0].clone();
    assert!(
        !h.engine().has_active_motion(),
        "a settled scene must not be asking for frames"
    );
    (h, moved, handle)
}

/// A handle reads where its element landed, in window coordinates.
#[test]
#[ntest::timeout(3_000)]
fn a_bound_handle_reports_where_layout_put_the_element() {
    let (h, _moved, handle) = mount();

    assert!(
        handle.is_bound(),
        "the handle must be bound once reconciled"
    );
    assert_eq!(
        handle.rect(),
        Some(h.located("box").absolute),
        "a handle and the engine must agree about the same box. They read the same layout, so a \
         disagreement means one of them is reading a different frame"
    );
}

/// A handle reports nothing rather than a zero rect when it cannot answer.
///
/// The distinction is what stops a first measurement being taken against the origin.
#[test]
#[ntest::timeout(3_000)]
fn an_unbound_handle_reports_nothing_rather_than_the_origin() {
    let orphan = ElementRef::new(Rc::new(RefCell::new(ReactiveRuntime::new())));
    assert!(
        orphan.rect().is_none() && orphan.position().is_none(),
        "an unbound handle must report nothing; a zero rect would be a position, and a client \
         measuring against it would animate out of the top-left corner"
    );
    assert!(!orphan.is_bound());
}

/// The FLIP animates: the element is caught between its two positions on some frame.
///
/// This is the assertion the previous index-based implementation could not make. Animating a slot
/// index and converting to pixels with a pitch makes the offset zero on the frame the swap lands,
/// so nothing is ever *in between* — and every other test in that suite still passed.
#[test]
#[ntest::timeout(3_000)]
fn a_moved_element_is_caught_in_flight_rather_than_teleporting() {
    let (mut h, moved, _handle) = mount();

    let before = h.located("box").absolute.x;
    moved.set(true);

    let mut first_frame_offset = None;
    let mut saw_mid_flight = false;
    for _ in 0..40 {
        h.step();
        let drawn = h.located("box").absolute.x;
        if first_frame_offset.is_none() {
            first_frame_offset = Some(h.transform("box").tx);
        }
        if drawn > before + 1.0 && drawn < before + 280.0 - 1.0 {
            saw_mid_flight = true;
        }
    }

    assert!(
        saw_mid_flight,
        "the element was never observed between its old and new positions: it teleported"
    );
    assert!(
        first_frame_offset.expect("a frame ran").abs() > 0.5,
        "the first frame after the move must already carry the inverted offset. If it does not, \
         the element is painted at its destination for one frame before springing back, which \
         reads as a flash"
    );
    assert!(h.settle(900), "the FLIP must come to rest");
    assert!(
        h.transform("box").tx.abs() < 0.5,
        "a settled FLIP must land on zero, not merely within the spring's tolerance: a residue \
         leaves the element permanently a fraction of a pixel from its slot"
    );
}

/// A FLIP must survive the rebuild that caused it.
///
/// The state lives in hook memory, so a rebuilt component finds its own. Without that, the rebuild
/// hands the row a fresh spring already sitting on its new position, which is precisely how a
/// FLIP degrades into a teleport.
#[test]
#[ntest::timeout(5_000)]
fn a_flip_survives_the_rebuild_that_caused_it() {
    let (mut h, moved, _handle) = mount();

    let before = h.located("box").absolute.x;
    moved.set(true);
    h.step();

    assert!(
        h.transform("box").tx.abs() > 0.5,
        "one frame after the move the offset is {}; a rebuilt spring would already be zero",
        h.transform("box").tx
    );
    assert!(h.settle(900));
    assert!(
        h.located("box").absolute.x > before + 100.0,
        "the element must end up where layout put it, not where it started"
    );
}

/// A resize moves every element without rearranging anything, and must animate nothing.
///
/// Position is not by itself a reason to animate. This is the case a pure position-diff gets
/// wrong: everything on screen moves and none of it was reordered.
#[test]
#[ntest::timeout(5_000)]
fn a_resize_does_not_animate_a_layout_that_did_not_reorder() {
    let (mut h, _moved, _handle) = mount();

    h.engine().resize(600, 320);
    h.step();
    h.step();

    assert!(
        h.transform("box").tx.abs() < 0.01,
        "the window changed shape and the element moved with it, but nothing was rearranged. \
         Animating that would make a resize look like a reorder; offset was {}",
        h.transform("box").tx
    );
    assert!(h.settle(900));
}

/// Repeated moves must not accumulate per-rebuild work.
///
/// A transition is never retired, because its target may move again at any time. Registering one
/// per rebuild therefore grows the registry without bound — invisibly, until a long session starts
/// dropping frames.
#[test]
#[ntest::timeout(5_000)]
fn repeated_moves_do_not_accumulate_post_layout_work() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let moved = Signal::new(Rc::clone(&rt), false);
    let refs = Rc::new(RefCell::new(Vec::new()));
    let for_tree = moved.clone();
    let tree = Cx::new(Rc::clone(&rt)).enter({
        let refs = Rc::clone(&refs);
        move || Slider(for_tree.clone(), refs)
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), tree, 400, 200);
    assert!(h.settle(600));

    let before = rt.borrow().post_layout_count();
    for i in 0..40 {
        moved.set(i % 2 == 0);
        h.step();
        h.step();
    }
    let after = rt.borrow().post_layout_count();

    assert_eq!(
        after, before,
        "forty moves must not add post-layout work; the count went {before} -> {after}"
    );
}

/// Reads must be inert: measuring an element cannot make the engine ask for another frame.
///
/// This is the property that separates a pull from a push. Publishing a measurement into a signal
/// invalidates that signal's subscribers, and an invalidated subscriber is itself what asks for
/// frames — so a scene that measures itself cannot settle. A handle has no such edge.
#[test]
#[ntest::timeout(5_000)]
fn reading_geometry_does_not_ask_the_engine_for_frames() {
    let (mut h, _moved, handle) = mount();

    for _ in 0..100 {
        let _ = handle.rect();
        let _ = handle.position();
        let _ = handle.center();
        let _ = handle.child_rects();
    }

    assert!(
        !h.engine().has_active_motion(),
        "a hundred reads asked for a frame. A read that dirties a node can schedule the frame \
         that invalidates the read, and the two feed each other"
    );
}

/// `child_rects` is how a list learns its own geometry without declaring a pitch.
#[test]
#[ntest::timeout(5_000)]
fn a_container_reports_its_children_in_child_order() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let list_ref = ElementRef::new(Rc::clone(&rt));

    let tree = Cx::new(Rc::clone(&rt)).enter({
        let list_ref = list_ref.clone();
        move || {
            group()
                .key("list")
                .width(Size::fill())
                .height(Size::fill())
                .direction(Direction::Vertical)
                .bind_ref(list_ref)
                .children([child("a", 40.0), child("b", 70.0), child("c", 55.0)])
                .into_element()
        }
    });
    let mut h = Harness::from_runtime(Rc::clone(&rt), tree, 300, 300);
    assert!(h.settle(600));

    let reported = list_ref.child_rects();
    assert_eq!(reported.len(), 3, "three children must be reported");

    for (name, height) in [("a", 40.0), ("b", 70.0), ("c", 55.0)] {
        assert_eq!(
            reported.iter().find(|r| r.height == height).copied(),
            Some(h.located(name).absolute),
            "child {name}: a container's report must match the engine's own reading"
        );
    }

    // Order is the property that makes this usable as a slot table at all.
    let ys: Vec<f32> = reported.iter().map(|r| r.y).collect();
    let mut sorted = ys.clone();
    sorted.sort_by(f32::total_cmp);
    assert_eq!(
        ys, sorted,
        "children must be reported top to bottom, in child order"
    );
}

fn child(name: &'static str, height: f32) -> Element {
    group()
        .key(name)
        .width(Size::fill())
        .height(height)
        .fill(Color::hex(0x111827))
        .into_element()
}
