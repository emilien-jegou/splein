// Single responsibility: `node_absolute_rect` reports the axis-aligned envelope of what is drawn.
//
// The absolute bounds query paired the node's absolute origin with its *local* size. That is only
// correct while the accumulated transform is a pure translation. As soon as a node is rotated or
// scaled the answer was wrong twice over: the box sat in the wrong place and had the wrong extent.
// A caller using it to aim a click or place an overlay would then target empty space.
//
// The paint pass already answers this question correctly via `compute_visual_bounds`, so the fix
// is to ask it rather than recompute the box by hand.

use riode::dsl::*;
use riode::foundation::{Color, ResolvedRect, Transform};
use riode_tiny_skia::testing::Harness;

fn harness(child: impl IntoElement) -> Harness {
    let mut h = Harness::root(
        group()
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .children([child]),
        400,
        400,
    );
    h.step();
    h
}

fn bounds(h: &mut Harness) -> ResolvedRect {
    h.engine().with_arena(|arena, root| {
        let child = arena.children(root)[0];
        arena.node_absolute_rect(child)
    })
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.5
}

#[test]
#[ntest::timeout(5_000)]
fn an_untransformed_box_reports_its_own_rect() {
    let mut h = harness(
        group()
            .key("probe")
            .width(100.0)
            .height(40.0)
            .fill(Color::hex(0xFF0000)),
    );
    let r = bounds(&mut h);
    // Envelope includes the 1px AA fringe rasterizers paint outside the
    // geometric box (see LayoutNode::AA_PAD).
    assert!(approx(r.x, -1.0) && approx(r.y, -1.0), "origin was {:?}", r);
    assert!(
        approx(r.width, 102.0) && approx(r.height, 42.0),
        "size was {:?}",
        r
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_translated_box_reports_its_offset_origin() {
    let mut h = harness(
        group()
            .key("probe")
            .width(100.0)
            .height(40.0)
            .fill(Color::hex(0xFF0000))
            .transform(Transform::from_translation(30.0, 20.0)),
    );
    let r = bounds(&mut h);
    assert!(approx(r.x, 29.0) && approx(r.y, 19.0), "origin was {:?}", r);
    assert!(
        approx(r.width, 102.0) && approx(r.height, 42.0),
        "size was {:?}",
        r
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_scaled_box_reports_the_grown_extent() {
    let mut h = harness(
        group()
            .key("probe")
            .width(100.0)
            .height(40.0)
            .fill(Color::hex(0xFF0000))
            .transform(Transform::from_scale(2.0, 3.0)),
    );
    let r = bounds(&mut h);
    assert!(
        approx(r.width, 202.0) && approx(r.height, 122.0),
        "a scaled box must report its scaled extent, got {r:?}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn a_rotated_box_reports_its_axis_aligned_envelope() {
    // 90 degrees: the 100x40 box becomes 40x100. Reporting the unrotated 100x40 here is exactly
    // the bug -- the origin is right but the extent is not.
    let mut h = harness(
        group()
            .key("probe")
            .width(100.0)
            .height(40.0)
            .fill(Color::hex(0xFF0000))
            .transform(Transform::from_rotation_degrees(90.0)),
    );
    let r = bounds(&mut h);
    assert!(
        approx(r.width, 42.0) && approx(r.height, 102.0),
        "a 90-degree rotation must swap the reported extent, got {r:?}"
    );
}

#[test]
#[ntest::timeout(5_000)]
fn the_reported_bounds_contain_the_painted_pixels() {
    // The property that actually matters: a click aimed using these bounds lands on the view.
    let mut h = harness(
        group()
            .key("probe")
            .width(100.0)
            .height(40.0)
            .fill(Color::hex(0xFF0000))
            .transform(Transform::from_rotation_degrees(90.0)),
    );
    let r = bounds(&mut h);
    let cx = r.x + r.width / 2.0;
    let cy = r.y + r.height / 2.0;
    assert_eq!(
        h.engine().hit_test(cx, cy),
        Some(riode::foundation::Key::new("probe")),
        "the centre of the reported bounds must hit the box"
    );
}
