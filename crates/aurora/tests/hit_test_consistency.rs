// Single responsibility: `Engine::hit_test` and the router's hit chain are the same answer.
//
// Hit resolution had two implementations. The router -- the path every real pointer event takes --
// used `input::hit::hit_chain`, which honours ancestor clips and walks children in reverse paint
// order. `Engine::hit_test` used `tree::hit_test::hit_test`, which ignored clips entirely and used
// document order. The two agreed on flat, unclipped, same-z trees, which is why the duplication
// went unnoticed, and disagreed on exactly the cases that are hard to debug by hand: a view
// clipped out of its parent, and a `z_index` sibling drawn above it.
//
// `Engine::hit_test` is the public entry point and the one tests use to aim, so a disagreement
// here is not a private inconsistency -- it sends tests and callers to the wrong view.

use aurora::dsl::*;
use aurora::foundation::{Color, Key, Point};
use aurora::input::hit::hit_chain;
use aurora::runtime::Engine;
use aurora_tiny_skia::testing::Harness;

/// The key the router's chain resolves at a point, i.e. what a real click would reach first.
fn chain_key(engine: &Engine, x: f32, y: f32) -> Option<Key> {
    engine.with_arena(|arena, root| {
        hit_chain(arena, root, Point::new(x, y))
            .into_iter()
            .find_map(|hit| arena.get(hit.id).key.clone())
    })
}

/// The key `Engine::hit_test` resolves at a point.
fn public_key(engine: &Engine, x: f32, y: f32) -> Option<Key> {
    engine.hit_test(x, y)
}

fn mounted(root: impl aurora::dsl::IntoElement) -> Engine {
    let mut engine = Engine::headless(400, 400);
    engine.mount(root);
    engine.frame();
    engine
}

/// A tall keyed child inside a short clipped parent. Only the parent's top strip is visible, so
/// the part of the child below the clip must not be clickable even though the child's own bounds
/// still cover that point.
fn clipped_child() -> impl aurora::dsl::IntoElement {
    group()
        .width(400.0)
        .height(400.0)
        .fill(Color::WHITE)
        .children([group()
            .width(200.0)
            .height(100.0)
            .clip(true)
            .children([group()
                .key("tall")
                .width(200.0)
                .height(400.0)
                .fill(Color::hex(0xEF4444))])])
}

/// Two overlapping keyed siblings where the second is explicitly stacked above the first.
///
/// `.absolute()` is required: the root lays out in a row by default, so without it the siblings
/// sit side by side, never overlap, and the test would pass without testing anything.
fn stacked_siblings() -> impl aurora::dsl::IntoElement {
    group()
        .width(400.0)
        .height(400.0)
        .fill(Color::WHITE)
        .children([
            group()
                .key("under")
                .absolute()
                .width(200.0)
                .height(200.0)
                .fill(Color::hex(0xEF4444)),
            group()
                .key("over")
                .absolute()
                .width(200.0)
                .height(200.0)
                .z_index(5)
                .fill(Color::hex(0x3B82F6)),
        ])
}

#[test]
#[ntest::timeout(30_000)]
fn a_view_clipped_out_of_its_parent_is_not_clickable() {
    let engine = mounted(clipped_child());

    // Inside the parent's 100px strip: the child is visible and clickable.
    assert_eq!(public_key(&engine, 50.0, 50.0), Some(Key::from("tall")));

    // Below the strip the child is clipped away, so nothing is under the point. The child's own
    // bounds still cover y=300, so an implementation that ignores clips returns "tall" here.
    assert_eq!(
        public_key(&engine, 50.0, 300.0),
        None,
        "a view clipped out of its parent must not be hit"
    );
}

#[test]
#[ntest::timeout(30_000)]
fn hit_test_agrees_with_the_router_on_a_clipped_view() {
    let engine = mounted(clipped_child());
    for y in [10.0, 50.0, 90.0, 150.0, 300.0, 390.0] {
        assert_eq!(
            public_key(&engine, 50.0, y),
            chain_key(&engine, 50.0, y),
            "the two hit-test paths disagreed at y={y}"
        );
    }
}

#[test]
#[ntest::timeout(30_000)]
fn hit_test_agrees_with_the_router_on_stacked_siblings() {
    let engine = mounted(stacked_siblings());
    for y in [10.0, 100.0, 190.0] {
        assert_eq!(
            public_key(&engine, 100.0, y),
            chain_key(&engine, 100.0, y),
            "the two hit-test paths disagreed at y={y}"
        );
    }
}

#[test]
#[ntest::timeout(30_000)]
fn the_topmost_stacked_sibling_is_the_hit_target() {
    let engine = mounted(stacked_siblings());
    assert_eq!(
        public_key(&engine, 100.0, 100.0),
        Some(Key::from("over")),
        "the sibling drawn last must receive the click"
    );
}

/// The invariant behind all of the above: the view that is clicked is the view that is painted.
///
/// Checking hit resolution against paint is the only way to catch an ordering rule that drifts in
/// one pipeline and not the other, and it is what a user actually notices. The two siblings here
/// fully overlap, so a click in the shared area must land on whichever one is visible.
#[test]
#[ntest::timeout(30_000)]
fn the_clicked_view_is_the_painted_view() {
    let mut h = Harness::root(stacked_siblings(), 400, 400);
    h.step();

    const BLUE: (u8, u8, u8) = (59, 130, 246);
    let c = h.view().color(100, 100);
    let painted_is_blue = (c.r, c.g, c.b) == BLUE;
    assert!(
        painted_is_blue,
        "the z=5 sibling must paint over the z=0 one, got {:?}",
        h.view().color(100, 100)
    );

    assert_eq!(
        h.engine().hit_test(100.0, 100.0),
        Some(Key::from("over")),
        "the click must land on the view that is actually on top"
    );
}
