// Single responsibility: A declared transform and bound pose components compose, not replace.
//
// The tree stores one `Transform` per node. Pose components (`.x() .y() .scale() .rotate_degrees()`)
// are composed into it and the declared `.transform()` folded in with it, so the two are not
// competing layers -- they are components of the same value.
//
// The two paths that write that value disagreed. Reconciliation composed them
// (`declared.multiply(&pose)`), but the reactive update path assigned the pose wholesale
// (`*transform = composed`), discarding the declared transform. So a node declared with both
// rendered correctly on mount and then silently lost its declared transform the first time a bound
// pose component changed. The bug only showed on the second frame, which is why no test caught it.

use aurora::dsl::*;
use aurora::foundation::Transform;
use aurora::prelude::*;
use aurora::reactive::{Cx, ReactiveRuntime, Signal};
use aurora_tiny_skia::testing::Harness;
use std::cell::RefCell;
use std::rc::Rc;

/// Reads the single stored transform off the first child of the root.
fn child_transform(h: &mut Harness) -> Transform {
    h.engine().with_arena(|arena, root| {
        let child = arena.children(root)[0];
        arena.get(child).transform
    })
}

#[test]
#[ntest::timeout(5_000)]
fn a_declared_transform_survives_a_bound_x_change() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let x = Signal::new(Rc::clone(&rt), 0.0f32);
    let mut h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(|| {
            group()
                .width(400.0)
                .height(400.0)
                .children([group()
                    .key("child")
                    .width(50.0)
                    .height(50.0)
                    .fill(Color::hex(0xFF0000))
                    .transform(Transform::from_translation(10.0, 20.0))
                    .x(x.clone())])
                .into_element()
        }),
        400,
        400,
    );
    h.step();

    // On mount, reconciliation composed the two: declared (10,20) then bound x (0).
    let mounted = child_transform(&mut h);
    assert_eq!(
        (mounted.tx, mounted.ty),
        (10.0, 20.0),
        "mount must compose the declared transform with the bound pose"
    );

    x.set(100.0);
    h.step();

    let moved = child_transform(&mut h);
    assert_eq!(
        (moved.tx, moved.ty),
        (110.0, 20.0),
        "after the bound x changes, the declared translation must still be there"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_declared_scale_survives_a_bound_x_change() {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let x = Signal::new(Rc::clone(&rt), 0.0f32);
    let mut h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(|| {
            group()
                .width(400.0)
                .height(400.0)
                .children([group()
                    .key("child")
                    .width(50.0)
                    .height(50.0)
                    .fill(Color::hex(0xFF0000))
                    .transform(Transform::from_scale(2.0, 2.0))
                    .x(x.clone())])
                .into_element()
        }),
        400,
        400,
    );
    h.step();
    x.set(100.0);
    h.step();

    let moved = child_transform(&mut h);
    assert_eq!(
        (moved.a, moved.d),
        (2.0, 2.0),
        "the declared scale must survive a reactive pose update, not be overwritten by it"
    );
}
