// Render benchmark: times full-frame layout+compile and raster replay headlessly.
//
// `cargo run --release --example render_bench`. Numbers only mean something
// compared across runs on the same machine; use it to gate raster changes.
use std::time::Instant;

use aurora::dsl::*;
use aurora::foundation::*;
use aurora::runtime::Engine;
use aurora_tiny_skia::HeadlessRaster;

fn page() -> impl IntoElement {
    group()
        .width(760.0)
        .height(660.0)
        .fill(Color::hex(0xF4F5F7))
        .clip(true)
        .children([
            group()
                .key("title")
                .width(300.0)
                .height(40.0)
                .children([text("Render benchmark harness").size(14.0).color(Color::BLACK)])
                .into_element(),
            group()
                .key("rows")
                .direction(Direction::Vertical)
                .width(760.0)
                .height(600.0)
                .gap(10.0)
                .children([
                    row("Baseline"),
                    row("Underline"),
                    row("Strikethrough"),
                    row("Highlight"),
                    row("Squiggle"),
                ])
                .into_element(),
        ])
}

fn row(label: &str) -> Element {
    group()
        .key(format!("row-{label}"))
        .direction(Direction::Horizontal)
        .width(760.0)
        .height(100.0)
        .radius(12.0)
        .fill(Color::hex(0x4F46E5))
        .shadows([Shadow::outer(0.0, 4.0, 8.0, Color::rgba(0.0, 0.0, 0.0, 0.5))])
        .children([text(label.to_string())
            .size(14.0)
            .color(Color::WHITE)
            .into_element()])
        .into_element()
}

fn main() {
    let mut engine = Engine::headless(760, 660);
    engine.load_font(include_bytes!("../assets/inter-var.ttf"));
    engine.mount(page());
    engine.frame();
    let mut raster = HeadlessRaster::new();
    raster.draw_engine(&mut engine);

    // Dirty one row to get realistic damage, then replay the same damage.
    let iters = 300u32;
    let t0 = Instant::now();
    for _ in 0..iters {
        raster.draw_engine(&mut engine);
    }
    let raster_ms = t0.elapsed().as_secs_f64() * 1000.0 / iters as f64;

    let t1 = Instant::now();
    for _ in 0..iters {
        engine.frame();
    }
    let frame_ms = t1.elapsed().as_secs_f64() * 1000.0 / iters as f64;

    println!("raster replay: {raster_ms:.3} ms/frame over {iters} iters");
    println!("engine frame : {frame_ms:.3} ms/frame over {iters} iters");
}
