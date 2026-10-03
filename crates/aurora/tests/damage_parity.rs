// Single responsibility: Retained damage-only canvas must equal a from-scratch repaint.

#[allow(dead_code)]
#[path = "../examples/motion_accordion/main.rs"]
mod accordion;

#[allow(dead_code)]
#[path = "../examples/motion_reorder.rs"]
mod reorder;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::dsl::IntoElement;
use aurora::foundation::{DamageRegion, ResolvedRect, Transform};
use aurora::reactive::{ReactiveRuntime, Signal};
use aurora::render::TinySkiaRenderer;
use aurora::runtime::Engine;

/// Pixels where the retained canvas differs from a full repaint, split by damage coverage.
fn stale_pixels(engine: &mut Engine) -> (u32, u32) {
    let retained = engine.canvas().clone();
    let (w, h) = engine.logical_size();

    let mut truth = TinySkiaRenderer::new();
    truth.set_text_context(engine.text_context());
    truth.resize(w, h);
    let mut full = DamageRegion::new();
    full.push(ResolvedRect::new(0.0, 0.0, w as f32, h as f32));
    truth.render_damage(engine.scene(), &full).expect("full repaint");
    let truth = truth.canvas();
    let damage = engine.current_damage().clone();

    let (mut covered, mut uncovered) = (0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            if retained.pixel(x, y) == truth.pixel(x, y) {
                continue;
            }
            // Mirror render_damage's floor/ceil tile, which is the region actually redrawn.
            let redrawn = damage.rects().iter().any(|d| {
                x as f32 >= d.x.floor()
                    && (x + 1) as f32 <= d.right().ceil()
                    && y as f32 >= d.y.floor()
                    && (y + 1) as f32 <= d.bottom().ceil()
            });
            if redrawn {
                covered += 1;
            } else {
                uncovered += 1;
            }
        }
    }
    (covered, uncovered)
}

/// Builds the accordion demo mounted at its own window size.
fn accordion_engine() -> (Engine, Signal<Option<usize>>) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), Some(0usize));
    let mut engine = Engine::with_runtime(
        runtime,
        accordion::build_ui(open.clone()).into_element(),
        accordion::WIDTH,
        accordion::HEIGHT,
    );
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    (engine, open)
}

#[test]
fn dragging_a_row_repaints_every_pixel_it_vacates() {
    let mut engine = Engine::headless(reorder::WIDTH, reorder::HEIGHT);
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine.mount(reorder::build_ui(&reorder::ITEMS));
    engine.frame();
    engine.canvas();

    let mut worst = 0u32;
    for f in 0..45 {
        let delta = f as f32 * 8.0;
        engine.update_motion("row-0", |motion| {
            motion.transform = Transform::from_translation(0.0, delta)
                .multiply(&Transform::from_scale(1.03, 1.03));
        });
        engine.frame();
        engine.canvas();
        let (_covered, uncovered) = stale_pixels(&mut engine);
        worst = worst.max(uncovered);
    }
    // Before clip-collapsed bounds were fixed this grew to 25632 (an uncleared drag trail).
    // What remains is a constant handful of anti-aliased corner pixels.
    assert!(worst <= 128, "drag left {worst} changed pixels outside the redrawn tiles");
}

#[test]
fn switching_accordion_panels_repaints_every_pixel_they_vacate() {
    let (mut engine, open) = accordion_engine();
    engine.frame();
    engine.canvas();

    let mut worst = 0u32;
    // Item 3's body overruns the content clip, which is the case that can strand pixels.
    for target in [3usize] {
        open.set(Some(target));
        for _f in 0..45 {
            engine.advance(1.0 / 60.0);
            engine.frame();
            engine.canvas();
            let (_covered, uncovered) = stale_pixels(&mut engine);
            worst = worst.max(uncovered);
        }
    }
    // A clip-collapsed bounds record used to strand the panel copy below the list.
    assert!(
        worst <= 128,
        "accordion left {worst} changed pixels outside the redrawn tiles"
    );
}
