// Single responsibility: the first-class `<AutoGrid>` element (P15) —
// chunking math through the component, rebuild-on-width-change, and keyed
// cells surviving a re-chunk. Numeric layout assertions only (row counts via
// cell widths); appearance belongs to the `wrap_repro` demo and human eyes.

use std::cell::RefCell;
use std::rc::Rc;

use riode::prelude::*;
use riode::internal::*; // P10 escape hatch: this test composes repr-builders directly
use riode::reactive::{Cx, ReactiveRuntime, Signal};
use riode::runtime::Engine;

#[component]
fn GridDemo(width: Signal<f32>, items: Signal<Vec<String>>) -> impl IntoElement {
    view! {
        <AutoGrid width={width} min_child={150.0} gap={8.0}>
            {items
                .get()
                .into_iter()
                .map(|s| {
                    group()
                        .key(s.clone())
                        .width("fill")
                        .height(48.0)
                        .into_element()
                })
                .collect::<Vec<_>>()}
        </AutoGrid>
    }
}

fn cell_widths(engine: &Engine, keys: &[&str]) -> Vec<f32> {
    keys.iter()
        .map(|k| {
            engine
                .rect(*k)
                .unwrap_or_else(|| panic!("cell {k} must exist"))
                .width
        })
        .collect()
}

fn drive() -> (Rc<RefCell<ReactiveRuntime>>, Signal<f32>, Signal<Vec<String>>, Engine) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let cx = Cx::new(Rc::clone(&runtime));
    let (width, items) = cx.enter(|| {
        (
            use_signal(420.0f32),
            use_signal(
                ["One", "Two", "Three", "Four", "Five", "Six"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
            ),
        )
    });
    let view: riode::dsl::Element =
        Cx::new(Rc::clone(&runtime)).enter(|| GridDemo(width, items.clone()));
    let mut engine = Engine::with_runtime(Rc::clone(&runtime), view, 800, 900);
    engine.advance(1.0 / 60.0);
    engine.frame();
    (runtime, width, items, engine)
}

#[test]
#[ntest::timeout(5_000)]
fn autogrid_chunks_six_cells_into_rows_of_two_at_420() {
    let (_rt, _width, _items, engine) = drive();
    // cols = floor((420 + 8) / (150 + 8)) = 2; child_w = (420 − 8) / 2 = 206.
    let keys = ["One", "Two", "Three", "Four", "Five", "Six"];
    for w in cell_widths(&engine, &keys) {
        assert!(
            (w - 206.0).abs() < 1.0,
            "each cell must be one 206px track wide, got {w}"
        );
    }
}

#[test]
#[ntest::timeout(5_000)]
fn autogrid_rechunks_on_width_change_and_keeps_keyed_cells() {
    let (_rt, width, _items, mut engine) = drive();
    // Widen: floor((760 + 8) / 158) = 4 tracks of (760 − 24) / 4 = 184.
    width.set(760.0);
    engine.advance(1.0 / 60.0);
    engine.frame();
    let keys = ["One", "Two", "Three", "Four", "Five", "Six"];
    for w in cell_widths(&engine, &keys) {
        assert!(
            (w - 184.0).abs() < 1.0,
            "after widening each cell must be one 184px track wide, got {w}"
        );
    }
    // Narrow: a single 160px track; every keyed cell survives the re-chunk.
    width.set(160.0);
    engine.advance(1.0 / 60.0);
    engine.frame();
    for w in cell_widths(&engine, &keys) {
        assert!(
            (w - 160.0).abs() < 1.0,
            "after narrowing each cell must fill the 160px track, got {w}"
        );
    }
}
