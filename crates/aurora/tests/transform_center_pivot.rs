// Single responsibility: Regression tests proving transforms pivot about a node's centre.

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::Engine;

/// Runs one frame and rasterizes its damage into the retained canvas.
fn step(engine: &mut Engine) {
    engine.frame();
    engine.canvas();
}

/// Reads one canvas pixel as an RGB triple.
fn rgb(engine: &mut Engine, x: f32, y: f32) -> (u8, u8, u8) {
    let p = engine
        .canvas()
        .pixel(x.floor() as u32, y.floor() as u32)
        .expect("pixel must exist");
    (p.red(), p.green(), p.blue())
}

/// Whether a pixel reads as the white page background.
fn is_white(c: (u8, u8, u8)) -> bool {
    c.0 > 250 && c.1 > 250 && c.2 > 250
}

/// Whether a pixel reads as the red subject box.
fn is_red(c: (u8, u8, u8)) -> bool {
    c.0 > 200 && c.1 < 120 && c.2 < 120
}

/// A white page holding one red box driven by `transform`.
fn page(transform: Transform) -> impl IntoElement {
    group()
        .width(200.0)
        .height(200.0)
        .fill(Color::WHITE)
        .children([group()
            .key("box")
            .width(100.0)
            .height(100.0)
            .fill(Color::hex(0xEF4444))
            .transform(transform)])
}

#[test]
fn a_declarative_scale_pivots_about_the_node_centre() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(page(Transform::from_scale(0.5, 0.5)));
    step(&mut engine);

    let r = engine.rect("box").expect("keyed box");
    let center = (r.x + r.width * 0.5, r.y + r.height * 0.5);
    let corner = (r.x + 2.0, r.y + 2.0);

    assert!(is_red(rgb(&mut engine, center.0, center.1)), "the shrunken box must cover its centre");
    assert!(
        is_white(rgb(&mut engine, corner.0, corner.1)),
        "an origin pivot would pull the box up into this corner"
    );
}

#[test]
fn a_declarative_rotation_keeps_the_node_in_place() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(page(Transform::from_rotation_degrees(180.0)));
    step(&mut engine);

    let r = engine.rect("box").expect("keyed box");
    let center = (r.x + r.width * 0.5, r.y + r.height * 0.5);

    assert!(
        is_red(rgb(&mut engine, center.0, center.1)),
        "a half turn about the centre must leave the box where it was"
    );
    assert!(
        is_white(rgb(&mut engine, r.x + r.width + 20.0, center.1)),
        "and must not swing into its neighbour's slot"
    );
}

#[test]
fn a_pure_translation_is_left_untouched() {
    let mut engine = Engine::headless(200, 200);
    engine.mount(page(Transform::from_translation(40.0, 0.0)));
    step(&mut engine);

    let r = engine.rect("box").expect("keyed box");
    assert!(is_white(rgb(&mut engine, 5.0, 5.0)), "the box must vacate its origin slot");
    assert!(
        is_red(rgb(&mut engine, r.x + r.width + 20.0, r.y + r.height * 0.5)),
        "translation must move the box without any centre compensation"
    );
}
