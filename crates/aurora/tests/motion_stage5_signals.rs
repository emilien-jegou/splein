// Single responsibility: TDD verification for signal-driven motion published by the motion clock.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::motion::Spring;
use aurora::runtime::Engine;

/// Advances forced frames, rasterizing each, until nothing wants more.
fn settle(engine: &mut Engine, limit: u32) {
    for _ in 0..limit {
        if !engine.has_active_motion() {
            break;
        }
        engine.advance(1.0 / 60.0);
        step(engine);
    }
}

#[test]
fn spring_signal_settles_and_stops_requesting_frames() {
    let mut engine = Engine::headless(400, 400);
    engine.mount(
        group().width(400.0).height(400.0).fill(Color::WHITE),
    );
    engine.frame();

    let progress = engine.spring_signal(Spring::snappy(), 0.0, 1.0);
    assert!(engine.has_active_motion(), "a published spring must want frames");
    assert_eq!(progress.get(), 0.0, "the signal starts at its initial value");

    settle(&mut engine, 400);

    assert!(!engine.has_active_motion(), "a settled spring signal must stop");
    let final_value = progress.get();
    assert!(
        (final_value - 1.0).abs() < 0.01,
        "expected the signal to settle at 1.0, got {final_value}"
    );
}

#[test]
fn one_spring_signal_drives_several_properties() {
    let mut engine = Engine::headless(400, 400);
    let progress = engine.spring_signal(Spring::snappy(), 0.0, 1.0);

    engine.mount(
        group().width(400.0).height(400.0).fill(Color::WHITE).children([
            group()
                .key("card")
                .width(100.0)
                .height(100.0)
                .fill(Color::hex(0xEF4444))
                .transform(progress.map(|p| Transform::from_translation(p * 200.0, 0.0)))
                .opacity(progress.clone()),
        ]),
    );
    step(&mut engine);
    assert!(
        is_white(rgb(&mut engine, 210, 10)),
        "the card starts at the origin, not at x=200"
    );

    settle(&mut engine, 400);

    let (r, g, b) = rgb(&mut engine, 210, 10);
    assert!(r > 200 && g < 120 && b < 120, "one signal must place and un-fade the card, got ({r},{g},{b})");
    assert!(
        is_white(rgb(&mut engine, 10, 10)),
        "the card must have left its origin"
    );
}

/// Runs one frame and rasterizes its damage into the retained canvas.
fn step(engine: &mut Engine) {
    engine.frame();
    engine.canvas();
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
