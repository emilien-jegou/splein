// Single responsibility: Diagnostic captures of the accordion switch, as labelled filmstrips.

use std::cell::RefCell;
use std::rc::Rc;

use aurora::foundation::Color;
use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use aurora::testing::{assert_disjoint, report, Filmstrip, Harness};

#[allow(dead_code)]
#[path = "../examples/motion_accordion/main.rs"]
mod accordion;

/// Steps the demo through a panel switch, capturing a labelled filmstrip and scanning for
/// geometry escapes at each frame.
///
/// The scan is the assertion; the strip is for a human to look at. Running the scan at every
/// step is what turns "it looked fine at frame 39" into a statement about the whole switch.
#[test]
#[ntest::timeout(30_000)]
fn capture_an_accordion_switch() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), Some(0usize));
    let cx = Cx::new(Rc::clone(&runtime));

    let mut h = Harness::from_runtime(
        Rc::clone(&runtime),
        cx.enter(|| accordion::Page(open.clone())),
        accordion::WIDTH,
        accordion::HEIGHT,
    )
    .with_background(Color::hex(0xF4F5F7));
    h.steps(6);

    // Switch panels by clicking the second header, the way a user would.
    h.click("item-1");

    let mut frames = Vec::new();
    for target in 0..=40u64 {
        h.step_to(target);
        // The invariant that matters for a stack of cards: they must never share pixels,
        // at any point in the switch.
        let cards: Vec<(String, _)> = (0..4)
            .map(|i| {
                let k = format!("panel-{i}");
                let here = h.located(&k);
                (k, here.absolute)
            })
            .collect();
        let named: Vec<(&str, _)> = cards.iter().map(|(k, r)| (k.as_str(), *r)).collect();
        assert_disjoint(&named);

        if matches!(target, 0 | 3 | 8 | 16 | 28 | 40) {
            frames.push((target, h.view()));
        }
    }

    Filmstrip::new(frames).save("/tmp/opencode/acc-switch-strip.png");

    h.settle(600);
    let notes = report(h.engine());
    if !notes.is_empty() {
        println!(
            "geometry notes (informational):\n  {}",
            notes
                .iter()
                .map(|e| e.describe())
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    }
}

/// Captures a single settled frame at full size, for looking at detail.
#[test]
#[ntest::timeout(30_000)]
fn capture_a_settled_accordion_frame() {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), Some(2usize));
    let cx = Cx::new(Rc::clone(&runtime));
    let mut h = Harness::from_runtime(
        Rc::clone(&runtime),
        cx.enter(|| accordion::Page(open.clone())),
        accordion::WIDTH,
        accordion::HEIGHT,
    )
    .with_background(Color::hex(0xF4F5F7));
    h.settle(600);

    h.screenshot("/tmp/opencode/acc-settled.png");

    let cards: Vec<(String, _)> = (0..4)
        .map(|i| {
            let k = format!("panel-{i}");
            (k.clone(), h.located(&k).absolute)
        })
        .collect();
    let named: Vec<(&str, _)> = cards.iter().map(|(k, r)| (k.as_str(), *r)).collect();
    assert_disjoint(&named);
    for i in 0..4 {
        let here = h.located(&format!("panel-{i}"));
        println!(
            "panel-{i}: flow={:?} render={:?} abs={:?}",
            here.flow, here.render, here.absolute
        );
    }
    println!("open signal = {:?}", open.get());
}
