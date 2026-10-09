// Single responsibility: Interactive shadow playground with grabbable sliders.
//
// One preview card driven by a reactive web_like shadow; four sliders
// (offset X/Y, blur, opacity) plus a blend toggle and a live CSS readout
// for direct comparison with the browser.

#[path = "common/mod.rs"]
mod common;

use aurora::prelude::*;
use aurora::reactive::ReactiveRuntime;
use common::ExampleCli;
use std::cell::RefCell;
use std::rc::Rc;

const TRACK_W: f32 = 200.0;
const TRACK_H: f32 = 28.0;
const THUMB_W: f32 = 14.0;

/// A labeled slider bound to `value` over `[min, max]`.
///
/// Relative dragging: the gesture anchors on grab, then maps horizontal
/// travel to the range, so no geometry reads are needed.
fn slider(
    rt: &Rc<RefCell<ReactiveRuntime>>,
    label: &'static str,
    value: Signal<f32>,
    min: f32,
    max: f32,
    fmt: Rc<dyn Fn(f32) -> String>,
) -> Element {
    let start_val = Signal::new(Rc::clone(rt), value.get());
    let start_x = Signal::new(Rc::clone(rt), 0.0f32);
    let thumb_x = computed(move || {
        let frac = ((value.get() - min) / (max - min)).clamp(0.0, 1.0);
        frac * (TRACK_W - THUMB_W)
    });
    let readout = computed({
        let fmt = Rc::clone(&fmt);
        move || fmt(value.get())
    });
    group()
        .direction(Direction::Vertical)
        .gap(4.0)
        .children((
            group()
                .direction(Direction::Horizontal)
                .gap(8.0)
                .alignment(Alignment::Center)
                .children((
                    text(label).size(13.0).color(Color::hex(0x333333)),
                    text(readout).size(13.0).color(Color::hex(0x111111)),
                )),
            group()
                .width(TRACK_W)
                .height(TRACK_H)
                .radius(Radius::scalar(6.0))
                .fill(Color::hex(0xD8D5D0))
                .hit_test(HitTest::Opaque)
                .on_drag_start({
                    let (value, start_val, start_x) = (value.clone(), start_val.clone(), start_x.clone());
                    move |info: &DragInfo| {
                        start_val.set(value.get());
                        start_x.set(info.position.x);
                    }
                })
                .on_drag_step(move |info: &DragInfo| {
                    let travel = info.position.x - start_x.get();
                    value.set((start_val.get() + travel * (max - min) / TRACK_W).clamp(min, max));
                })
                .children([group()
                    .width(THUMB_W)
                    .height(20.0)
                    .radius(Radius::scalar(7.0))
                    .fill(Color::hex(0x483050))
                    .x(thumb_x)
                    // Static pose values are dropped by the reconciler;
                    // positions must be closures.
                    .y(move || 4.0)
                    .hit_test(HitTest::Passthrough)]),
        ))
        .into_element()
}

fn main() {
    let cli = ExampleCli::parse(980, 620);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mk = |v: f32| Signal::new(Rc::clone(&runtime), v);
    let (ox, oy, blur, opacity) = (mk(3.0), mk(6.0), mk(6.0), mk(0.35));
    let multiply = Signal::new(Rc::clone(&runtime), false);

    let shadow = {
        let (ox, oy, blur, opacity, multiply) = (
            ox.clone(),
            oy.clone(),
            blur.clone(),
            opacity.clone(),
            multiply.clone(),
        );
        computed(move || {
            let s = Shadow::web_like(ox.get(), oy.get(), blur.get(), Color::rgba(0.0, 0.0, 0.0, opacity.get()));
            if multiply.get() { s.multiply() } else { s }
        })
    };
    let css = {
        let (ox, oy, blur, opacity) = (ox.clone(), oy.clone(), blur.clone(), opacity.clone());
        computed(move || {
            format!(
                "box-shadow: {:.1}px {:.1}px {:.1}px rgba(0, 0, 0, {:.2});",
                ox.get(),
                oy.get(),
                blur.get(),
                opacity.get()
            )
        })
    };
    let toggle_label = {
        let multiply = multiply.clone();
        computed(move || {
            if multiply.get() {
                "blend: multiply".to_string()
            } else {
                "blend: normal".to_string()
            }
        })
    };

    cli.app_config("Aurora — Shadow Studio")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(0xE8E6E1))
        .runtime(Rc::clone(&runtime))
        .run(move || {
            let f1: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.1}px"));
            let f2: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.1}px"));
            let f3: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.1}px"));
            let f4: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.2}"));
            let rt = Rc::clone(&runtime);
            group()
                .direction(Direction::Horizontal)
                .width(Size::fill())
                .height(Size::fill())
                .fill(Color::hex(0xE8E6E1))
                .alignment(Alignment::Center)
                .children([
                    // Control panel.
                    group()
                        .direction(Direction::Vertical)
                        .gap(14.0)
                        .margin(Margin::all(28.0))
                        .children((
                            slider(&rt, "offset X", ox.clone(), -40.0, 40.0, f1),
                            slider(&rt, "offset Y", oy.clone(), -40.0, 40.0, f2),
                            slider(&rt, "blur", blur.clone(), 0.0, 40.0, f3),
                            slider(&rt, "opacity", opacity.clone(), 0.0, 1.0, f4),
                            group()
                                .width(150.0)
                                .height(34.0)
                                .radius(Radius::scalar(8.0))
                                .fill(Color::hex(0x483050))
                                .alignment(Alignment::Center)
                                .distribution(Distribution::Center)
                                .hit_test(HitTest::Opaque)
                                .on_click({
                                    let multiply = multiply.clone();
                                    move |_| multiply.set(!multiply.get())
                                })
                                .children([text(toggle_label.clone())
                                    .size(13.0)
                                    .color(Color::WHITE)]),
                            text(css.clone()).size(12.0).color(Color::hex(0x555555)),
                        )),
                    // Preview stage.
                    group()
                        .width(Size::fill())
                        .height(Size::fill())
                        .fill(Color::hex(0xF1CFFC))
                        .alignment(Alignment::Center)
                        .distribution(Distribution::Center)
                        .children([group()
                            .width(150.0)
                            .height(150.0)
                            .radius(Radius::scalar(8.0))
                            .fill(Color::WHITE)
                            .dynamic_shadow(shadow.clone())]),
                ])
        });
}
