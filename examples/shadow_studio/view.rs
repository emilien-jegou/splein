// Single responsibility: Interactive playground covering the whole shadow DSL.
// View only (`Page` component, no `main`): the release driver (`main.rs`)
// and the hot entry (`hot.rs`) share this; the switchable studio reuses
// `boot` for in-window switching.
//
// Modes: web polyfill (offset/blur/spread/opacity), physical elevation
// (elevation/tint/falloff/spread), and layered drop stacks. Sliders drive
// reactive signals; a live readout prints the equivalent construction for
// direct comparison with the browser or the cheat sheet.

use riode::app::switcher::{BootCtx, BootedDemo};
use riode::prelude::*;
use riode::reactive::{DynamicProp, ReactiveRuntime};
use std::cell::RefCell;
use std::rc::Rc;

/// Window chrome shared by the release driver (`main.rs`) and the hot entry
/// (`hot.rs`): declared once here, referenced by both — never mirrored.
pub const HOT_TITLE: &str = "Aurora — Shadow Studio";
pub const HOT_WIDTH: u32 = 980;
pub const HOT_HEIGHT: u32 = 700;
pub const HOT_BG: u32 = 0xE8E6E1;

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
    let track_value = value;
    let step_value = value;
    let start = start_val;
    let start_x_handle = start_x;
    view! {
        <Group direction="vertical" gap={4.0}>
            <Group direction="horizontal" gap={8.0} alignment="center">
                <Text size={13.0} color={0x333333}>{label}</Text>
                <Text size={13.0} color={0x111111}>{readout}</Text>
            </Group>
            <Group
                width={TRACK_W}
                height={TRACK_H}
                radius={6.0}
                fill={0xD8D5D0}
                hit_test={HitTest::Opaque}
                on:drag-start={{
                    move |info: &DragInfo| {
                        start_val.set(track_value.get());
                        start_x.set(info.position.x);
                    }
                }}
                on:drag-step={{
                    move |info: &DragInfo| {
                        let travel = info.position.x - start_x_handle.get();
                        step_value.set(
                            (start.get() + travel * (max - min) / TRACK_W).clamp(min, max),
                        );
                    }
                }}
            >
                <Group
                    width={THUMB_W}
                    height={20.0}
                    radius={7.0}
                    fill={0x483050}
                    x={thumb_x}
                    y={4.0}
                    hit_test={HitTest::Passthrough}
                />
            </Group>
        </Group>
    }
    .into_element()
}

/// A small labeled button.
fn button(label: DynamicProp<String>, on_press: impl Fn() + 'static) -> Element {
    view! {
        <Group
            width={150.0}
            height={34.0}
            radius={8.0}
            fill={0x483050}
            alignment="center"
            distribution="center"
            hit_test={HitTest::Opaque}
            on:click={{
                move |_| on_press()
            }}
        >
            <Text size={13.0} color={Color::WHITE}>{label}</Text>
        </Group>
    }
}

/// Every signal the studio owns, created explicitly on a runtime (`boot`) or
/// from hooks (`Page`). One struct so both entries build the same world.
#[derive(Clone)]
struct Params {
    ox: Signal<f32>,
    oy: Signal<f32>,
    blur: Signal<f32>,
    spread: Signal<f32>,
    opacity: Signal<f32>,
    elevation: Signal<f32>,
    tint: Signal<Color>,
    mode: Signal<i32>,
    falloff_idx: Signal<i32>,
    multiply: Signal<bool>,
    alpha_shape: Signal<bool>,
}

/// Explicit-signal construction for runtimes owned outside a component
/// (the switchable studio and the standalone release driver).
fn params_on(runtime: &Rc<RefCell<ReactiveRuntime>>) -> Params {
    let mk = |v: f32| Signal::new(Rc::clone(runtime), v);
    Params {
        ox: mk(3.0),
        oy: mk(6.0),
        blur: mk(6.0),
        spread: mk(0.0),
        opacity: mk(0.35),
        elevation: mk(12.0),
        tint: Signal::new(Rc::clone(runtime), Color::BLACK),
        mode: Signal::new(Rc::clone(runtime), 0i32),
        falloff_idx: Signal::new(Rc::clone(runtime), 0i32),
        multiply: Signal::new(Rc::clone(runtime), false),
        alpha_shape: Signal::new(Rc::clone(runtime), false),
    }
}

/// The studio UI from already-created signals. Runs inside any build scope:
/// `boot` wraps it in `enter_keyed`, `Page` is a component scope already.
fn ui(p: Params, rt: Rc<RefCell<ReactiveRuntime>>) -> Element {
    let Params {
        ox,
        oy,
        blur,
        spread,
        opacity,
        elevation,
        tint,
        mode,
        falloff_idx,
        multiply,
        alpha_shape,
    } = p;
    let apply_common = |s: Shadow, multiply: bool, alpha_shape: bool| -> Shadow {
        let s = if multiply { s.multiply() } else { s.normal() };
        if alpha_shape {
            s.alpha()
        } else {
            s
        }
    };
    let shadow = {
        let (ox, oy, blur, spread, opacity, elevation, tint) = (
            ox,
            oy,
            blur,
            spread,
            opacity,
            elevation,
            tint,
        );
        let (mode, falloff_idx, multiply, alpha_shape) = (
            mode,
            falloff_idx,
            multiply,
            alpha_shape,
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
        let mode = mode;
        computed(move || match mode.get() {
            0 => "mode: web".to_string(),
            1 => "mode: elevation".to_string(),
            _ => "mode: layered".to_string(),
        })
    };
    let css = {
        let (ox, oy, blur, spread, opacity, elevation, tint) = (
            ox,
            oy,
            blur,
            spread,
            opacity,
            elevation,
            tint,
        );
        let (mode, falloff_idx) = (mode, falloff_idx);
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

    let fpx: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.1}px"));
    let fop: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.2}"));
    let in_web = computed({
        let mode = mode;
        move || mode.get() == 0
    });
    let in_elev = computed({
        let mode = mode;
        move || mode.get() == 1
    });
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height="fill"
            fill={HOT_BG}
            alignment="center"
        >
            // Control panel.
            <Group direction="vertical" gap={12.0} margin={28.0}>
                {button(mode_name.clone(), {
                    let mode = mode;
                    move || mode.set((mode.get() + 1) % 3)
                })}
                <Group direction="vertical" gap={12.0} visible={in_web.clone()}>
                    {slider(&rt, "offset X", ox, -40.0, 40.0, fpx.clone())}
                    {slider(&rt, "offset Y", oy, -40.0, 40.0, fpx.clone())}
                    {slider(&rt, "blur", blur, 0.0, 40.0, fpx.clone())}
                </Group>
                <Group direction="vertical" gap={12.0} visible={in_elev.clone()}>
                    {slider(
                        &rt,
                        "elevation",
                        elevation,
                        0.0,
                        32.0,
                        fpx.clone(),
                    )}
                </Group>
                {slider(&rt, "spread", spread, -12.0, 24.0, fpx.clone())}
                {slider(&rt, "opacity", opacity, 0.0, 1.0, fop.clone())}
                <Group direction="horizontal" gap={8.0}>
                    {swatch(tint, Color::BLACK)}
                    {swatch(tint, 0x7A3D8F)}
                    {swatch(tint, 0x2563EB)}
                </Group>
                {button(
                    {
                        let falloff_idx = falloff_idx;
                        computed(move || {
                            format!(
                                "falloff: {}",
                                FALLOFFS[falloff_idx.get().clamp(0, 2) as usize].1
                            )
                        })
                    },
                    {
                        let falloff_idx = falloff_idx;
                        move || falloff_idx.set((falloff_idx.get() + 1) % 3)
                    },
                )}
                {button(
                    {
                        let multiply = multiply;
                        computed(move || {
                            if multiply.get() {
                                "blend: multiply".to_string()
                            } else {
                                "blend: normal".to_string()
                            }
                        })
                    },
                    {
                        let multiply = multiply;
                        move || multiply.set(!multiply.get())
                    },
                )}
                {button(
                    {
                        let alpha_shape = alpha_shape;
                        computed(move || {
                            if alpha_shape.get() {
                                "shape: alpha".to_string()
                            } else {
                                "shape: box".to_string()
                            }
                        })
                    },
                    {
                        let alpha_shape = alpha_shape;
                        move || alpha_shape.set(!alpha_shape.get())
                    },
                )}
                // Fold: the panel stays fit to the sliders while this line
                // folds at whatever width the siblings decide — no fixed
                // width, no panel growth as the readout lengthens.
                <Text size={12.0} color={0x555555} width="fold">{css.clone()}</Text>
            </Group>
            // Preview stage.
            <Group
                width="fill"
                height="fill"
                fill={0xF1CFFC}
                alignment="center"
                distribution="center"
            >
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Group
                        width={150.0}
                        height={150.0}
                        radius={8.0}
                        fill={Color::WHITE}
                        shadow={shadow.clone()}
                    />
                    <Group
                        light={LightEnvironment::from_angles(270.0, 45.0)}
                        direction="vertical"
                        gap={8.0}
                        alignment="center"
                    >
                        <Group
                            width={110.0}
                            height={110.0}
                            radius={8.0}
                            fill={Color::WHITE}
                            shadow={12.0}
                        />
                        <Text size={12.0} color={0x483050}>"light az 270"</Text>
                    </Group>
                </Group>
            </Group>
        </Group>
    }
    .into_element()
}

/// Rebuilds the studio world on a fresh runtime for in-window switching.
pub(crate) fn boot(ctx: &BootCtx) -> BootedDemo {
    let runtime = Rc::clone(&ctx.runtime);
    let p = params_on(&runtime);
    let cx = Cx::new(Rc::clone(&runtime));
    let root = cx
        .enter_keyed("root", move || {
            let rt = Rc::clone(&runtime);
            ui(p, rt)
        })
        .into_element();
    BootedDemo {
        root,
        background: HOT_BG.into(),
        extensions: vec![],
    }
}

/// A tint swatch button writing `color` (alpha preserved from current).
fn swatch(tint: Signal<Color>, color: impl Into<Color>) -> Element {
    let color = color.into();
    view! {
        <Group
            width={36.0}
            height={28.0}
            radius={6.0}
            fill={color}
            hit_test={HitTest::Opaque}
            on:click={{
                move |_| {
                    let a = tint.get().a;
                    tint.set(color.with_alpha(a));
                }
            }}
        />
    }
}

/// Hot entry component: same world, hook-owned signals.
#[component]
pub fn Page() -> impl IntoElement {
    let p = Params {
        ox: use_signal(3.0),
        oy: use_signal(6.0),
        blur: use_signal(6.0),
        spread: use_signal(0.0),
        opacity: use_signal(0.35),
        elevation: use_signal(12.0),
        tint: use_signal(Color::BLACK),
        mode: use_signal(0i32),
        falloff_idx: use_signal(0i32),
        multiply: use_signal(false),
        alpha_shape: use_signal(false),
    };
    let rt = p.ox.runtime();
    ui(p, rt)
}
