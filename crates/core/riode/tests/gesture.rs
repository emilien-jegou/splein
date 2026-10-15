// Single responsibility: the Drag gesture primitive's press/track/release
// state machine, driven through platform events headlessly (no window).
//
// P16: pins that `on:drag={Drag::start(…).step(…).end(…)}` fans out to the
// five typed slots with release cleanup guaranteed — grab on press, steps
// past the drag threshold, end with velocity on release, and *no* step/end
// for a press that never becomes a drag. Motion fidelity itself belongs to
// `motion_reorder` and human eyes.

use std::cell::RefCell;
use std::rc::Rc;

use riode::app::runner::pointer::PointerBridge;
use riode::dsl::Element;
use riode::foundation::geometry::Point;
use riode::prelude::*;
use riode::reactive::{Cx, ReactiveRuntime, Signal};
use riode_tiny_skia::testing::Harness;
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};

fn pos(x: f64, y: f64) -> winit::dpi::PhysicalPosition<f64> {
    winit::dpi::PhysicalPosition::new(x, y)
}

fn moved(x: f64, y: f64) -> WindowEvent {
    WindowEvent::CursorMoved {
        position: pos(x, y),
        device_id: unsafe { DeviceId::dummy() },
    }
}

fn button(state: ElementState, button: MouseButton) -> WindowEvent {
    WindowEvent::MouseInput {
        state,
        button,
        device_id: unsafe { DeviceId::dummy() },
    }
}

#[component]
fn DragPad(at: Signal<Option<Point>>, log: Signal<Vec<String>>) -> impl IntoElement {
    view! {
        <Group key="pad" width={400.0} height={400.0} on:drag={
            Drag::start(at)
                .grab({
                    let log = log.clone();
                    move |_| {
                        log.update(|v| v.push("grab".to_string()));
                    }
                })
                .step({
                    let log = log.clone();
                    move |_| {
                        log.update(|v| v.push("step".to_string()));
                    }
                })
                .end({
                    let log = log.clone();
                    move |_| {
                        // Read before updating: nesting a `get` inside the
                        // `update` closure double-borrows the runtime.
                        // The displacement must still be live while end runs:
                        // release glides seed from it, and clearing first
                        // would seed them from zero (snap instead of glide).
                        let live = at.get().is_some();
                        log.update(|v| {
                            v.push(if live { "end-live" } else { "end-stale" }.to_string())
                        });
                        log.update(|v| v.push("end".to_string()));
                    }
                })
        } />
    }
}

fn pad() -> (
    Harness,
    Signal<Option<Point>>,
    Signal<Vec<String>>,
) {
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let at = Signal::new(Rc::clone(&runtime), None);
    let log = Signal::new(Rc::clone(&runtime), Vec::new());
    let view: Element =
        Cx::new(Rc::clone(&runtime)).enter(|| DragPad(at.clone(), log.clone()));
    let h = Harness::from_runtime(Rc::clone(&runtime), view, 400, 400);
    (h, at, log)
}

#[test]
#[ntest::timeout(5_000)]
fn press_tracks_release_clears_and_end_fires() {
    let (mut h, at, log) = pad();
    h.steps(2);
    let mut bridge = PointerBridge::new();

    bridge.dispatch(&moved(200.0, 200.0), h.engine());
    bridge.dispatch(
        &button(ElementState::Pressed, MouseButton::Left),
        h.engine(),
    );
    h.steps(2);
    assert_eq!(log.get(), vec!["grab".to_string()]);
    // A press is not a drag yet: grab bookkeeping ran, but no position is
    // latched — latching here would engage followers with the full grab
    // offset on the first held frame (a visible jump).
    assert_eq!(at.get(), None, "press alone must not latch a position");

    // Past the drag threshold: steps stream live positions.
    bridge.dispatch(&moved(260.0, 200.0), h.engine());
    h.steps(2);
    assert!(log.get().contains(&"step".to_string()));
    assert!(
        matches!(at.get(), Some(p) if p.x == 260.0),
        "moves must track the pointer"
    );

    bridge.dispatch(
        &button(ElementState::Released, MouseButton::Left),
        h.engine(),
    );
    h.steps(4);
    assert!(log.get().contains(&"end".to_string()));
    assert!(
        log.get().contains(&"end-live".to_string()),
        "end must run while the displacement is still live"
    );
    assert_eq!(at.get(), None, "release must clear the position");
}

#[test]
#[ntest::timeout(5_000)]
fn tap_without_travel_fires_no_step_or_end() {
    let (mut h, at, log) = pad();
    h.steps(2);
    let mut bridge = PointerBridge::new();

    bridge.dispatch(&moved(200.0, 200.0), h.engine());
    bridge.dispatch(
        &button(ElementState::Pressed, MouseButton::Left),
        h.engine(),
    );
    bridge.dispatch(
        &button(ElementState::Released, MouseButton::Left),
        h.engine(),
    );
    h.steps(4);
    // Grab ran (press bookkeeping), but a press that never became a drag
    // fires neither step nor end — and leaves no stale hold behind.
    assert_eq!(log.get(), vec!["grab".to_string()]);
    assert_eq!(at.get(), None);
}
