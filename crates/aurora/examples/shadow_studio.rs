// Single responsibility: Interactive playground covering the whole shadow DSL.
//
// Modes: web polyfill (offset/blur/spread/opacity), physical elevation
// (elevation/tint/falloff/spread), and layered drop stacks. Sliders drive
// reactive signals; a live readout prints the equivalent construction for
// direct comparison with the browser or the cheat sheet.

#[path = "common/mod.rs"]
mod common;

use aurora::prelude::*;
use aurora::reactive::DynamicProp;
use aurora::reactive::ReactiveRuntime;
use common::ExampleCli;
use std::cell::RefCell;
use std::rc::Rc;

const TRACK_W: f32 = 200.0;
const TRACK_H: f32 = 28.0;
const THUMB_W: f32 = 14.0;

const FALLOFFS: [(FalloffCurve, &str); 3] = [
    (FalloffCurve::EaseOut, "ease-out"),
    (FalloffCurve::Physical, "physical"),
    (FalloffCurve::Linear, "linear"),
];

const STACK: [(f32, f32, f32); 5] = [
    (0.5, 1.0, 1.0),
    (1.0, 2.0, 2.0),
    (2.0, 4.0, 4.0),
    (4.0, 8.0, 8.0),
    (8.0, 16.0, 16.0),
];

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

/// A small labeled button.
fn button(label: DynamicProp<String>, on_press: impl Fn() + 'static) -> Element {
    group()
        .width(150.0)
        .height(34.0)
        .radius(Radius::scalar(8.0))
        .fill(Color::hex(0x483050))
        .alignment(Alignment::Center)
        .distribution(Distribution::Center)
        .hit_test(HitTest::Opaque)
        .on_click(move |_| on_press())
        .children([text(label).size(13.0).color(Color::WHITE)])
        .into_element()
}

fn main() {
    let cli = ExampleCli::parse(980, 700);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let mk = |v: f32| Signal::new(Rc::clone(&runtime), v);
    let (ox, oy, blur, spread, opacity, elevation) =
        (mk(3.0), mk(6.0), mk(6.0), mk(0.0), mk(0.35), mk(12.0));
    let tint = Signal::new(Rc::clone(&runtime), Color::BLACK);
    let mode = Signal::new(Rc::clone(&runtime), 0i32);
    let falloff_idx = Signal::new(Rc::clone(&runtime), 0i32);
    let multiply = Signal::new(Rc::clone(&runtime), false);
    let alpha_shape = Signal::new(Rc::clone(&runtime), false);

    let apply_common = |s: Shadow, multiply: bool, alpha_shape: bool| -> Shadow {
        let s = if multiply { s.multiply() } else { s.normal() };
        if alpha_shape { s.alpha() } else { s }
    };
    let shadow = {
        let (ox, oy, blur, spread, opacity, elevation, tint) = (
            ox.clone(),
            oy.clone(),
            blur.clone(),
            spread.clone(),
            opacity.clone(),
            elevation.clone(),
            tint.clone(),
        );
        let (mode, falloff_idx, multiply, alpha_shape) = (
            mode.clone(),
            falloff_idx.clone(),
            multiply.clone(),
            alpha_shape.clone(),
        );
        computed(move || {
            let tinted = tint.get().with_alpha(opacity.get());
            let falloff = FALLOFFS[falloff_idx.get().clamp(0, 2) as usize].0;
            let mult = multiply.get();
            let alpha = alpha_shape.get();
            let built = match mode.get() {
                // Web polyfill: exact CSS parameters.
                0 => Shadow::web_like(ox.get(), oy.get(), blur.get(), spread.get(), tinted),
                // Physical elevation with derived physics.
                1 => Shadow::elevation(elevation.get())
                    .spread(spread.get())
                    .color(tinted)
                    .falloff(falloff),
                // Layered drop stack.
                _ => Shadow::layered(tinted)
                    .drops(&STACK)
                    .spread(spread.get())
                    .falloff(falloff),
            };
            apply_common(built, mult, alpha)
        })
    };
    let mode_name = {
        let mode = mode.clone();
        computed(move || {
            match mode.get() {
                0 => "mode: web".to_string(),
                1 => "mode: elevation".to_string(),
                _ => "mode: layered".to_string(),
            }
        })
    };
    let css = {
        let (ox, oy, blur, spread, opacity, elevation, tint) = (
            ox.clone(),
            oy.clone(),
            blur.clone(),
            spread.clone(),
            opacity.clone(),
            elevation.clone(),
            tint.clone(),
        );
        let (mode, falloff_idx) = (mode.clone(), falloff_idx.clone());
        computed(move || match mode.get() {
            0 => format!(
                "box-shadow: {:.1}px {:.1}px {:.1}px {:.1}px rgba(0, 0, 0, {:.2});",
                ox.get(),
                oy.get(),
                blur.get(),
                spread.get(),
                opacity.get()
            ),
            1 => format!(
                "elevation({:.1}).spread({:.1}).color({:?}).falloff({:?})",
                elevation.get(),
                spread.get(),
                tint.get(),
                FALLOFFS[falloff_idx.get().clamp(0, 2) as usize].1,
            ),
            _ => format!(
                "layered({:?}).drops(5 layers).spread({:.1})",
                tint.get(),
                spread.get(),
            ),
        })
    };

    cli.app_config("Aurora — Shadow Studio")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::hex(0xE8E6E1))
        .runtime(Rc::clone(&runtime))
        .run(move || {
            let fpx: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.1}px"));
            let fop: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.2}"));
            let rt = Rc::clone(&runtime);
            let in_web = computed({
                let mode = mode.clone();
                move || mode.get() == 0
            });
            let in_elev = computed({
                let mode = mode.clone();
                move || mode.get() == 1
            });
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
                        .gap(12.0)
                        .margin(Margin::all(28.0))
                        .children((
                            button(mode_name.clone(), {
                                let mode = mode.clone();
                                move || mode.set((mode.get() + 1) % 3)
                            }),
                            group()
                                .direction(Direction::Vertical)
                                .gap(12.0)
                                .visible(in_web.clone())
                                .children((
                                    slider(&rt, "offset X", ox.clone(), -40.0, 40.0, fpx.clone()),
                                    slider(&rt, "offset Y", oy.clone(), -40.0, 40.0, fpx.clone()),
                                    slider(&rt, "blur", blur.clone(), 0.0, 40.0, fpx.clone()),
                                )),
                            group()
                                .direction(Direction::Vertical)
                                .gap(12.0)
                                .visible(in_elev.clone())
                                .children([slider(
                                    &rt,
                                    "elevation",
                                    elevation.clone(),
                                    0.0,
                                    32.0,
                                    fpx.clone(),
                                )]),
                            slider(&rt, "spread", spread.clone(), -12.0, 24.0, fpx.clone()),
                            slider(&rt, "opacity", opacity.clone(), 0.0, 1.0, fop.clone()),
                            group()
                                .direction(Direction::Horizontal)
                                .gap(8.0)
                                .children([
                                    swatch(tint.clone(), Color::BLACK),
                                    swatch(tint.clone(), Color::hex(0x7A3D8F)),
                                    swatch(tint.clone(), Color::hex(0x2563EB)),
                                ]),
                            button(
                                {
                                    let falloff_idx = falloff_idx.clone();
                                    computed(move || {
                                        format!(
                                            "falloff: {}",
                                            FALLOFFS[falloff_idx.get().clamp(0, 2) as usize].1
                                        )
                                    })
                                },
                                {
                                    let falloff_idx = falloff_idx.clone();
                                    move || falloff_idx.set((falloff_idx.get() + 1) % 3)
                                },
                            ),
                            button(
                                {
                                    let multiply = multiply.clone();
                                    computed(move || {
                                        if multiply.get() {
                                            "blend: multiply".to_string()
                                        } else {
                                            "blend: normal".to_string()
                                        }
                                    })
                                },
                                {
                                    let multiply = multiply.clone();
                                    move || multiply.set(!multiply.get())
                                },
                            ),
                            button(
                                {
                                    let alpha_shape = alpha_shape.clone();
                                    computed(move || {
                                        if alpha_shape.get() {
                                            "shape: alpha".to_string()
                                        } else {
                                            "shape: box".to_string()
                                        }
                                    })
                                },
                                {
                                    let alpha_shape = alpha_shape.clone();
                                    move || alpha_shape.set(!alpha_shape.get())
                                },
                            ),
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
                            .direction(Direction::Horizontal)
                            .gap(48.0)
                            .alignment(Alignment::Center)
                            .children((
                                group()
                                    .width(150.0)
                                    .height(150.0)
                                    .radius(Radius::scalar(8.0))
                                    .fill(Color::WHITE)
                                    .shadow(shadow.clone()),
                                group()
                                    .light(LightEnvironment::from_angles(270.0, 45.0))
                                    .direction(Direction::Vertical)
                                    .gap(8.0)
                                    .alignment(Alignment::Center)
                                    .children((
                                        group()
                                            .width(110.0)
                                            .height(110.0)
                                            .radius(Radius::scalar(8.0))
                                            .fill(Color::WHITE)
                                            .shadow(12.0),
                                        text("light az 270")
                                            .size(12.0)
                                            .color(Color::hex(0x483050)),
                                    )),
                            ))]),
                ])
        });
}

/// A tint swatch button writing `color` (alpha preserved from current).
fn swatch(tint: Signal<Color>, color: Color) -> Element {
    group()
        .width(36.0)
        .height(28.0)
        .radius(Radius::scalar(6.0))
        .fill(color)
        .hit_test(HitTest::Opaque)
        .on_click(move |_| {
            let a = tint.get().a;
            tint.set(color.with_alpha(a));
        })
        .into_element()
}
