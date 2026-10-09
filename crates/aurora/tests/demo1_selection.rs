// Regression tests: demo1's selectable text through the real PointerBridge
// path, at 1x and at the physical size HiDPI windows start with.

#[path = "../examples/demo1.rs"]
mod demo1;

use std::cell::RefCell;
use std::rc::Rc;

use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};

use aurora::app::runner::pointer::PointerBridge;
use aurora::foundation::Point;
use aurora::prelude::*;
use aurora::reactive::{Cx, ReactiveRuntime};
use aurora::text::TextSelection;
use aurora_tiny_skia::testing::{Harness, Rgba};

fn moved(x: f64, y: f64) -> WindowEvent {
    WindowEvent::CursorMoved {
        position: winit::dpi::PhysicalPosition::new(x, y),
        device_id: unsafe { DeviceId::dummy() },
    }
}

fn button(state: ElementState) -> WindowEvent {
    WindowEvent::MouseInput {
        state,
        button: MouseButton::Left,
        device_id: unsafe { DeviceId::dummy() },
    }
}

fn hello_rect(h: &mut Harness) -> aurora::foundation::ResolvedRect {
    h.engine().with_arena(|arena, root| {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !arena.is_valid(id) {
                continue;
            }
            let node = arena.get(id);
            if let aurora::tree::NodeKind::Text(t) = &node.kind {
                if t.content == "Hello" {
                    let p = arena.node_absolute_point(id);
                    let layout = node.state.text.cached_text_layout.clone().expect("shaped");
                    let w = layout.total_size.width;
                    let hgt = layout.total_size.height;
                    return aurora::foundation::ResolvedRect::new(p.x, p.y, w, hgt);
                }
            }
            stack.extend(arena.children(id).iter().copied());
        }
        panic!("Hello not found");
    })
}

fn hello_selection(h: &mut Harness) -> Option<TextSelection> {
    h.engine().with_arena(|arena, root| {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !arena.is_valid(id) {
                continue;
            }
            let node = arena.get(id);
            if let aurora::tree::NodeKind::Text(t) = &node.kind {
                if t.content == "Hello" {
                    return node.state.text.text_selection;
                }
            }
            stack.extend(arena.children(id).iter().copied());
        }
        None
    })
}

fn mount() -> (Harness, PointerBridge) {
    let rt = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let h = Harness::from_runtime(
        Rc::clone(&rt),
        Cx::new(Rc::clone(&rt)).enter(demo1::build_ui),
        demo1::WIDTH,
        demo1::HEIGHT,
    );
    (h, PointerBridge::new())
}

#[test]
fn demo1_press_sets_caret_through_bridge() {
    let (mut h, mut bridge) = mount();
    h.steps(10);
    let r = hello_rect(&mut h);
    let at = Point::new(r.x + 4.0, r.y + r.height * 0.5);
    bridge.dispatch(&moved(at.x as f64, at.y as f64), h.engine());
    h.step();
    bridge.dispatch(&button(ElementState::Pressed), h.engine());
    h.step();
    let sel = hello_selection(&mut h);
    assert!(sel.is_some(), "press must set a caret");
}

#[test]
fn demo1_drag_selects_range_through_bridge() {
    let (mut h, mut bridge) = mount();
    h.steps(10);
    let r = hello_rect(&mut h);
    let a = Point::new(r.x + 2.0, r.y + r.height * 0.5);
    let b = Point::new(r.x + r.width - 2.0, r.y + r.height * 0.5);
    bridge.dispatch(&moved(a.x as f64, a.y as f64), h.engine());
    h.step();
    bridge.dispatch(&button(ElementState::Pressed), h.engine());
    h.step();
    for i in 1..=6 {
        let t = i as f64 / 6.0;
        bridge.dispatch(
            &moved(a.x as f64 + (b.x as f64 - a.x as f64) * t, a.y as f64),
            h.engine(),
        );
        h.step();
    }
    // Damage must cover the highlight on the drag frame itself: input-only
    // paint dirties bypass the reactive drains, and the outbox carries them
    // to the damage plan. (Release frames are legitimately quiet.)
    assert!(
        !h.engine().current_damage().rects().is_empty(),
        "selection frame must damage"
    );
    bridge.dispatch(&button(ElementState::Released), h.engine());
    h.step();
    let sel = hello_selection(&mut h);
    assert!(sel.is_some_and(|s| !s.is_collapsed()), "drag must select, got {sel:?}");
    // The highlight must reach pixels, not just engine state: selection blue
    // rgba(0, 0.47, 0.84, 0.30) over the 0x4900FF box ≈ (22, 36, 242).
    let highlight = Rgba { r: 22, g: 36, b: 242, a: 255 };
    let is_highlight = |p: Rgba| p.near(highlight, 40);
    let before = {
        // Baseline from a fresh mount: stroke AA must not match, or the
        // assertion below is vacuous.
        let (mut h0, _) = mount();
        h0.steps(10);
        h0.view().count_matching(1, is_highlight)
    };
    let n = h.view().count_matching(1, is_highlight);
    // The baseline is non-zero (stroke/glyph AA near the tint), so what
    // matters is the jump the highlight strip adds (~45x20px here).
    assert!(
        n > before + 200,
        "highlight must paint (baseline {before}, after {n})"
    );
}

#[test]
fn hidpi_startup_size_keeps_physical_input_on_target() {
    // Real windows run physical pixels end to end (viewport from `Resized`,
    // input from `CursorMoved`, backend surface): on a 2x display the engine
    // is 1000x964 and presses land at 2x coordinates. `App::run` seeds that
    // size from `window.inner_size()` at startup so the first press — before
    // any `Resized` arrives — already hits. Without the seed the engine sits
    // at the 500x482 logical size and every physical press misses.
    let (mut h, mut bridge) = mount();
    h.steps(10);
    // Simulate the fixed startup: engine takes the window's physical size.
    h.engine().resize(demo1::WIDTH * 2, demo1::HEIGHT * 2);
    h.steps(10);
    let r = hello_rect(&mut h);
    let at = Point::new(r.x + 4.0, r.y + r.height * 0.5);
    bridge.dispatch(&moved(at.x as f64, at.y as f64), h.engine());
    h.step();
    bridge.dispatch(&button(ElementState::Pressed), h.engine());
    h.step();
    let sel = hello_selection(&mut h);
    assert!(sel.is_some(), "physical press must set a caret, got {sel:?}");
}
