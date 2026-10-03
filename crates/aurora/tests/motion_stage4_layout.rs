// Single responsibility: TDD verification for layout presence collapse and FLIP transitions.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::motion::{Ease, Millis, MotionVector, Spring, Transition, Tween};
use aurora::reactive::Signal;
use aurora::runtime::{Engine, FrameReport};

const GREEN: u32 = 0x22C55E;
const RED: u32 = 0xEF4444;
const BLUE: u32 = 0x3B82F6;

/// Runs one frame and rasterizes its damage into the retained canvas.
fn step(engine: &mut Engine) -> FrameReport {
    let report = engine.frame();
    engine.canvas();
    report
}

/// Advances forced frames until no controller wants more.
fn settle(engine: &mut Engine, limit: u32) {
    for _ in 0..limit {
        if !engine.has_active_motion() {
            break;
        }
        engine.advance(1.0 / 60.0);
        step(engine);
    }
}

/// Reads one pixel as an RGB triple from the engine's CPU canvas.
fn rgb(engine: &mut Engine, x: u32, y: u32) -> (u8, u8, u8) {
    let p = engine.canvas().pixel(x, y).expect("pixel must exist");
    (p.red(), p.green(), p.blue())
}

/// Whether a pixel reads as the white page background.
fn is_white(c: (u8, u8, u8)) -> bool {
    c.0 > 250 && c.1 > 250 && c.2 > 250
}

/// Whether a pixel reads as the blue tail box.
fn is_blue(c: (u8, u8, u8)) -> bool {
    c.2 > 200 && c.0 < 120
}

/// Whether a pixel reads as the red middle box.
fn is_red(c: (u8, u8, u8)) -> bool {
    c.0 > 200 && c.1 < 120 && c.2 < 120
}

/// Builds three stacked 100px boxes whose middle one takes a presence signal.
fn stack(middle: Signal<LayoutPresence>, transition: Option<Transition>) -> impl IntoElement {
    let column = group()
        .direction(Direction::Vertical)
        .width(400.0)
        .height(400.0)
        .fill(Color::WHITE)
        .children((
            group().width(100.0).height(100.0).fill(Color::hex(GREEN)),
            group()
                .width(100.0)
                .height(100.0)
                .fill(Color::hex(RED))
                .layout_presence(middle),
            group().width(100.0).height(100.0).fill(Color::hex(BLUE)),
        ));
    match transition {
        Some(t) => column.layout_transition(t),
        None => column,
    }
}

#[test]
fn absent_node_collapses_and_siblings_follow_immediately() {
    let mut engine = Engine::headless(400, 400);
    let presence = engine.signal(LayoutPresence::Present);
    engine.mount(stack(presence.clone(), None));
    step(&mut engine);

    assert!(is_red(rgb(&mut engine, 10, 150)), "the middle box must be present");
    assert!(is_blue(rgb(&mut engine, 10, 250)), "the tail box must sit below it");

    presence.set(LayoutPresence::Absent);
    step(&mut engine);

    assert!(!engine.has_active_motion(), "without a transition nothing animates");
    assert!(is_blue(rgb(&mut engine, 10, 150)), "the tail box must close the gap");
    assert!(is_white(rgb(&mut engine, 10, 250)), "the vacated tail slot must clear");
}

#[test]
fn placeholder_keeps_its_footprint() {
    let mut engine = Engine::headless(400, 400);
    let presence = engine.signal(LayoutPresence::Present);
    engine.mount(stack(presence.clone(), None));
    step(&mut engine);

    presence.set(LayoutPresence::Placeholder);
    step(&mut engine);

    assert!(is_red(rgb(&mut engine, 10, 150)), "a placeholder keeps rendering");
    assert!(
        is_blue(rgb(&mut engine, 10, 250)),
        "a placeholder must not shift siblings"
    );
}

#[test]
fn layout_transition_springs_a_shifted_sibling_without_popping() {
    let mut engine = Engine::headless(400, 400);
    let presence = engine.signal(LayoutPresence::Present);
    engine.mount(stack(presence.clone(), Some(Transition::Spring(Spring::snappy()))));
    step(&mut engine);

    presence.set(LayoutPresence::Absent);
    step(&mut engine);

    assert!(
        engine.has_active_motion(),
        "a sibling whose rect moved must start a FLIP spring"
    );
    assert!(
        is_blue(rgb(&mut engine, 10, 250)),
        "the sibling must still present its previous position on the first frame"
    );
    assert!(
        is_white(rgb(&mut engine, 10, 150)),
        "the sibling must not have travelled yet"
    );

    settle(&mut engine, 240);

    assert!(!engine.has_active_motion(), "the FLIP spring must settle");
    assert!(is_blue(rgb(&mut engine, 10, 150)), "the sibling must settle at its new rect");
    assert!(is_white(rgb(&mut engine, 10, 250)), "the vacated slot must clear");
}

#[test]
fn expanding_node_grows_through_its_transition() {
    let mut engine = Engine::headless(400, 400);
    let presence = engine.signal(LayoutPresence::Absent);
    engine.mount(stack(presence.clone(), Some(Transition::Spring(Spring::snappy()))));
    step(&mut engine);
    assert!(is_blue(rgb(&mut engine, 10, 150)), "the collapsed stack starts closed");

    presence.set(LayoutPresence::Present);
    step(&mut engine);

    assert!(engine.has_active_motion(), "an expanding box must spring open");
    assert!(
        is_blue(rgb(&mut engine, 10, 150)),
        "the tail box must hold its old rect on the expanding frame"
    );
    assert!(
        is_white(rgb(&mut engine, 10, 250)),
        "the expanding box must still be closed"
    );

    settle(&mut engine, 240);
    assert!(is_red(rgb(&mut engine, 10, 150)), "the box must grow into its rect");
    assert!(is_blue(rgb(&mut engine, 10, 250)), "the tail box must slide back down");
}

#[test]
fn flip_leaves_a_node_something_else_is_already_moving() {
    let mut engine = Engine::headless(400, 400);
    let presence = engine.signal(LayoutPresence::Present);
    engine.mount(
        group()
            .direction(Direction::Vertical)
            .width(400.0)
            .height(400.0)
            .fill(Color::WHITE)
            .layout_transition(Transition::Spring(Spring::snappy()))
            .children((
                group().width(100.0).height(100.0).fill(Color::hex(GREEN)),
                group()
                    .key("dragged")
                    .width(100.0)
                    .height(100.0)
                    .fill(Color::hex(RED))
                    .layout_presence(presence.clone()),
                group().width(100.0).height(100.0).fill(Color::hex(BLUE)),
            )),
    );
    step(&mut engine);

    engine.update_motion("dragged", |motion| {
        motion.transform = Transform::from_translation(0.0, 40.0);
    });
    step(&mut engine);

    // Collapsing the node moves its rect, which is exactly when FLIP would normally intervene.
    presence.set(LayoutPresence::Absent);
    step(&mut engine);

    let state = engine.motion_state("dragged").expect("motion persists");
    let owned = MotionVector::from_state(&state);
    assert!(
        (owned.y - 40.0).abs() < 1e-3,
        "FLIP must not spring a node another owner is moving, got y={}",
        owned.y
    );
}

#[test]
fn transition_scales_only_the_axis_that_changed() {
    let mut engine = Engine::headless(400, 400);
    let width = engine.signal(100.0f32);
    let column = group()
        .direction(Direction::Vertical)
        .width(400.0)
        .height(400.0)
        .fill(Color::WHITE)
        .layout_transition(Transition::Timed(Tween::new(400.ms(), Ease::Linear)))
        .children([group().width(width.clone()).height(100.0).fill(Color::hex(RED))]);
    engine.mount(column);
    step(&mut engine);
    assert!(is_red(rgb(&mut engine, 80, 50)), "the child starts 100px wide");

    width.set(60.0);
    step(&mut engine);

    assert!(engine.has_active_motion(), "a resizing child must FLIP");
    assert!(
        is_red(rgb(&mut engine, 80, 50)),
        "the child must invert to its previous 100px width"
    );
    assert!(
        is_white(rgb(&mut engine, 10, 105)),
        "height must not scale, only width changed"
    );

    settle(&mut engine, 240);
    assert!(is_white(rgb(&mut engine, 80, 50)), "the child must settle at its new width");
    assert!(is_red(rgb(&mut engine, 10, 50)), "the child must stay 100px tall");
}
