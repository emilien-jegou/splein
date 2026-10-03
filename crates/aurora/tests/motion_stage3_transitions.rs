// Single responsibility: TDD verification for declarative transitions on bound properties.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::motion::{Ease, Millis, Spring, Tween};
use aurora::reactive::computed;
use aurora::runtime::{Engine, FrameReport};
use aurora::tree::DirtyFlags;

/// Reads the green channel of a red-over-white blend, which tracks effective opacity.
fn blended_green(engine: &mut Engine) -> u8 {
    engine.canvas().pixel(50, 10).unwrap().green()
}

#[test]
fn bound_opacity_change_tweens_instead_of_snapping() {
    let mut engine = Engine::headless(400, 400);
    let opacity = engine.signal(1.0f32);

    engine.mount(
        group().width(400.0).height(400.0).fill(Color::WHITE).children([
            group()
                .width(100.0)
                .height(100.0)
                .fill(Color::RED)
                .opacity(opacity.clone())
                .animate(Tween::new(400.ms(), Ease::Linear)),
        ]),
    );
    engine.frame();
    assert_eq!(blended_green(&mut engine), 0, "fully opaque at rest");

    opacity.set(0.5);
    engine.frame();
    assert!(
        engine.has_active_motion(),
        "a bound change must start a transition"
    );
    assert_eq!(
        blended_green(&mut engine),
        0,
        "the transition must start from the previous value instead of snapping"
    );

    engine.advance(200.ms());
    engine.frame();
    let halfway = blended_green(&mut engine);
    assert!(
        (55..=75).contains(&halfway),
        "expected about 64 at half time, got {halfway}"
    );

    engine.advance(200.ms());
    engine.frame();
    let finished = blended_green(&mut engine);
    assert!(
        (118..=138).contains(&finished),
        "expected about 127 at completion, got {finished}"
    );
    assert!(!engine.has_active_motion(), "a settled transition must stop");
}

#[test]
fn declarative_opacity_renders_at_its_stated_value() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group().width(400.0).height(400.0).fill(Color::WHITE).children([
            group().width(100.0).height(100.0).fill(Color::RED).opacity(0.5),
        ]),
    );
    engine.frame();

    let green = blended_green(&mut engine);
    assert!(
        (118..=138).contains(&green),
        "a declared opacity of 0.5 must blend to about 127, not its square, got {green}"
    );
}

#[test]
fn bound_change_without_a_transition_commits_immediately() {
    let mut engine = Engine::headless(400, 400);
    let opacity = engine.signal(1.0f32);

    engine.mount(
        group().width(400.0).height(400.0).fill(Color::WHITE).children([
            group().width(100.0).height(100.0).fill(Color::RED).opacity(opacity.clone()),
        ]),
    );
    engine.frame();

    opacity.set(0.5);
    engine.frame();

    assert!(!engine.has_active_motion(), "no transition was declared");
    let green = blended_green(&mut engine);
    assert!(
        (118..=138).contains(&green),
        "an unterminated bound value must commit at once, got {green}"
    );
}

#[test]
fn spring_transition_settles_a_bound_property() {
    let mut engine = Engine::headless(400, 400);
    let opacity = engine.signal(1.0f32);

    engine.mount(
        group().width(400.0).height(400.0).fill(Color::WHITE).children([
            group()
                .width(100.0)
                .height(100.0)
                .fill(Color::RED)
                .opacity(opacity.clone())
                .spring(Spring::snappy()),
        ]),
    );
    engine.frame();

    opacity.set(0.4);
    engine.frame();
    for _ in 0..180 {
        if !engine.has_active_motion() {
            break;
        }
        engine.advance(1.0 / 60.0);
        engine.frame();
    }

    assert!(!engine.has_active_motion(), "a spring transition must settle");
    let green = blended_green(&mut engine);
    assert!(
        (143..=163).contains(&green),
        "expected about 153 for opacity 0.4, got {green}"
    );
}

#[test]
fn bound_transform_change_tweens_without_reflows() {
    let mut engine = Engine::headless(400, 400);
    let offset = engine.signal(0.0f32);

    engine.mount(
        group().width(400.0).height(400.0).fill(Color::WHITE).children([
            group()
                .width(100.0)
                .height(100.0)
                .fill(Color::RED)
                .transform({
                    let offset = offset.clone();
                    computed(move || Transform::from_translation(offset.get(), 0.0))
                })
                .animate(Tween::new(400.ms(), Ease::Linear)),
        ]),
    );
    engine.frame();
    assert_eq!(blended_green(&mut engine), 0, "the card starts at the origin");

    offset.set(100.0);
    let FrameReport { stats, diagnostics: diag, .. } = engine.frame();
    assert_eq!(stats.flags, DirtyFlags::PAINT, "a transition invalidates paint");
    assert!(!stats.laid_out, "a transition never reflows");
    assert!(diag.layout.recomputed_nodes.is_empty());

    engine.advance(200.ms());
    engine.frame();

    let canvas = engine.canvas();
    assert_eq!(
        canvas.pixel(110, 10).unwrap().green(),
        0,
        "the card must be halfway to its target"
    );
    assert_eq!(
        canvas.pixel(10, 10).unwrap().green(),
        255,
        "the vacated origin must be cleared"
    );
}
