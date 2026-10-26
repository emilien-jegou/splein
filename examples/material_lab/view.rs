// Material lab: live instrument for the custom-stage mechanism (G0) and
// the macro-time DSL (G1). Layout mirrors `glass_studio`: a scrollable
// control sidebar left, one big editable card right.
//
// The card wears exactly one stack: the macro-time DSL surface from the
// sidebar sliders (auto-stamped neutral `Glass::geometry()` host +
// post-glass custom shading stage — D-M4). Dragging a slider refills one
// uniform slot; the generated WGSL source never changes (pipeline cache
// must hit — D-M5).
//
// Per AGENTS.md: predictions live below for the human to check, not
// claims. Drag every slider end to end: shading may move, shape must
// not (rounded corners, silhouette — the structure/value split).

use riode::foundation::{Color, ColorStop, Fill, LinearGradient, Point};
use riode::prelude::*;
use riode::reactive::{ReactiveRuntime, computed};
use std::cell::RefCell;
use std::rc::Rc;

/// Window chrome shared by the release driver (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both.
pub const HOT_TITLE: &str = "Material lab";
pub const HOT_WIDTH: u32 = 1100;
pub const HOT_HEIGHT: u32 = 700;
pub const HOT_BG: u32 = 0x14161C;

const TRACK_W: f32 = 200.0;
const TRACK_H: f32 = 28.0;
const THUMB_W: f32 = 14.0;

const BG_NAMES: [&str; 4] = ["white", "gray", "black", "stripes"];

/// Stage grounds for the background cycler: three flats for reading the
/// sheen on light / mid / dark, plus hard-stop color stripes (the old
/// TINT food) for checking shading uniformity across hues.
fn bg_fill(idx: i32) -> Fill {
    match idx.rem_euclid(4) {
        0 => Fill::solid(Color::rgb(0.92, 0.92, 0.94)),
        1 => Fill::solid(Color::rgb(0.5, 0.5, 0.52)),
        2 => Fill::solid(Color::rgb(0.06, 0.06, 0.08)),
        _ => Fill::LinearGradient(LinearGradient {
            start: Point::new(0.0, 0.0),
            end: Point::new(640.0, 0.0),
            stops: vec![
                ColorStop {
                    position: 0.0,
                    color: Color::rgb(0.9, 0.9, 0.9),
                },
                ColorStop {
                    position: 0.2,
                    color: Color::rgb(0.9, 0.9, 0.9),
                },
                ColorStop {
                    position: 0.2,
                    color: Color::rgb(0.85, 0.2, 0.2),
                },
                ColorStop {
                    position: 0.4,
                    color: Color::rgb(0.85, 0.2, 0.2),
                },
                ColorStop {
                    position: 0.4,
                    color: Color::rgb(0.2, 0.7, 0.3),
                },
                ColorStop {
                    position: 0.6,
                    color: Color::rgb(0.2, 0.7, 0.3),
                },
                ColorStop {
                    position: 0.6,
                    color: Color::rgb(0.2, 0.4, 0.9),
                },
                ColorStop {
                    position: 0.8,
                    color: Color::rgb(0.2, 0.4, 0.9),
                },
                ColorStop {
                    position: 0.8,
                    color: Color::rgb(0.9, 0.9, 0.9),
                },
                ColorStop {
                    position: 1.0,
                    color: Color::rgb(0.9, 0.9, 0.9),
                },
            ],
        }),
    }
}

/// All live state, one signal per control. `bg` picks the stage ground
/// (0 white / 1 gray / 2 black / 3 stripes); the rest feed the DSL
/// surface's uniform slots.
#[derive(Clone)]
struct Params {
    bg: Signal<i32>,
    roughness: Signal<f32>,
    tint_r: Signal<f32>,
    tint_g: Signal<f32>,
    tint_b: Signal<f32>,
    gain: Signal<f32>,
    gloss: Signal<f32>,
    hover_gloss: Signal<bool>,
}

/// A labeled slider bound to `value` over `[min, max]`, with a one-line
/// `about` saying what the control is expected to do.
///
/// Relative dragging (glass_studio pattern): the gesture anchors on grab,
/// then maps horizontal travel to the range, so no geometry reads needed.
fn slider(
    rt: &Rc<RefCell<ReactiveRuntime>>,
    label: &'static str,
    about: &'static str,
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
    let readout = {
        let fmt = Rc::clone(&fmt);
        move || fmt(value.get())
    };
    let track_value = value;
    let step_value = value;
    let start = start_val;
    let start_x_handle = start_x;
    view! {
        <Group direction="vertical" gap={4.0}>
            <Group direction="horizontal" gap={8.0} alignment="center">
                <Text size={13.0} color={0x9AA0AE}>{label}</Text>
                <Text size={13.0} color={0xFFFFFF}>{readout}</Text>
            </Group>
            <Text size={11.0} color={0x6B7280} width="fold">{about}</Text>
            <Group
                width={TRACK_W}
                height={TRACK_H}
                radius={6.0}
                fill={0x2A2E38}
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
                    fill={0x7DD3FC}
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
fn button(label: Prop<String>, on_press: impl Fn() + 'static) -> Element {
    view! {
        <Group
            width={200.0}
            height={34.0}
            radius={8.0}
            fill={0x2563EB}
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

fn ui(p: Params, rt: Rc<RefCell<ReactiveRuntime>>) -> Element {
    // Stage ground follows the `bg` cycler: a `move || Fill` closure
    // (`IntoProp<Fill>` blanket → Dynamic prop, re-evaluated under the
    // paint subscriber, so taps repaint).
    let bg = p.bg;
    let stage_fill = move || bg_fill(bg.get());
    // Readout through one `computed` (folded, so the panel keeps the
    // sliders' width as it lengthens).
    let readout = {
        let p = p.clone();
        computed(move || {
            format!(
                "bg {} · rough {:.2} · tint ({:.2}, {:.2}, {:.2}) · gain {:.2} · gloss {:.2}{}",
                BG_NAMES[p.bg.get().rem_euclid(4) as usize],
                p.roughness.get(),
                p.tint_r.get(),
                p.tint_g.get(),
                p.tint_b.get(),
                p.gain.get(),
                p.gloss.get(),
                if p.hover_gloss.get() {
                    " · HOVER"
                } else {
                    ""
                },
            )
        })
    };
    let f2: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.2}"));
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height="fill"
            fill={HOT_BG}
            alignment="center"
        >
            // Control sidebar: scrolls, so every control fits at 700 px.
            <ScrollView width={248.0} height="fill">
            <Group direction="vertical" gap={10.0} margin={24.0}>
                {button(
                    {
                        let bg = p.bg;
                        computed(move || {
                            format!(
                                "background: {} (tap: next)",
                                BG_NAMES[bg.get().rem_euclid(4) as usize]
                            )
                        })
                    },
                    {
                        let p = p.clone();
                        move || p.bg.set((p.bg.get() + 1).rem_euclid(4))
                    },
                )}
                <Text size={11.0} color={0x6B7280} width="fold">{"stage ground: white / gray / black / color stripes for reading the sheen."}</Text>
                {slider(&rt, "roughness", "GGX lobe width: low = tight mirror highlight, high = broad matte sheen.", p.roughness, 0.02, 1.0, f2.clone())}
                {slider(&rt, "tint_r", "sheen red channel: the diffuse tint multiplied under the lobe.", p.tint_r, 0.0, 1.0, f2.clone())}
                {slider(&rt, "tint_g", "sheen green channel: drag to zero for a magenta cast.", p.tint_g, 0.0, 1.0, f2.clone())}
                {slider(&rt, "tint_b", "sheen blue channel: the demo default leans warm-red.", p.tint_b, 0.0, 1.0, f2.clone())}
                {slider(&rt, "env gain", "sky brightness into the tinted lobe: overall sheen level.", p.gain, 0.0, 2.0, f2.clone())}
                {slider(&rt, "gloss", "clearcoat strength: a second tight lobe layered over the base.", p.gloss, 0.0, 1.0, f2.clone())}
                {button(
                    {
                        let v = p.hover_gloss;
                        computed(move || {
                            if v.get() {
                                "hover: ON (tap: release)".to_string()
                            } else {
                                "hover: OFF (tap: press)".to_string()
                            }
                        })
                    },
                    {
                        let v = p.hover_gloss;
                        move || v.set(!v.get())
                    },
                )}
                <Text size={11.0} color={0x6B7280} width="fold">{"hover flips only the gloss slot 0.9 / slider: same source, values-only change."}</Text>
                <Text size={12.0} color={0x9AA0AE} width="fold">{readout}</Text>
            </Group>
            </ScrollView>
            // Preview stage: one big editable card wearing the DSL
            // surface INLINE in the `backdrop_effects` attribute — the
            // macro only intercepts material blocks there. Each slider
            // read is a plain numeric (value-level) filling one uniform
            // slot; drags refill values with byte-identical WGSL source
            // (cache hit — D-M5).
            <Group
                width="fill"
                height="fill"
                fill={stage_fill}
                clip={true}
                alignment="center"
                distribution="center"
            >
                <Group direction="vertical" gap={12.0} alignment="center">
                    <Group
                        width={420.0}
                        height={330.0}
                        radius={24.0}
                        backdrop_effects={
                            let rough = p.roughness.get();
                            let tr = p.tint_r.get();
                            let tg = p.tint_g.get();
                            let tb = p.tint_b.get();
                            let g = p.gain.get();
                            let gloss = if p.hover_gloss.get() { 0.9 } else { p.gloss.get() };
                            let n = Normal::flat();
                            let lobe = Microfacet::ggx(n, rough);
                            let tinted = Color::rgb(tr, tg, tb) * lobe;
                            let lit = tinted * Sky::gain(g);
                            let coat = Microfacet::ggx(n, 0.15) * Sky::gain(gloss);
                            let sheen = Color::rgb(0.55, 0.6, 0.7) * coat;
                            (lit + sheen).into_surface()
                        }
                        alignment="center"
                        distribution="center"
                    >
                        <Text size={72.0} weight={700u16} color={Color::WHITE}>{"Aa"}</Text>
                    </Group>
                    <Text size={12.0} color={0x6B7280}>{"drag any slider end to end: shading may move, shape must not."}</Text>
                </Group>
            </Group>
        </Group>
    }
    .into_element()
}

#[component]
pub fn Page() -> impl IntoElement {
    let p = Params {
        bg: use_signal(0i32),
        roughness: use_signal(0.3),
        tint_r: use_signal(1.0),
        tint_g: use_signal(0.25),
        tint_b: use_signal(0.2),
        gain: use_signal(1.5),
        gloss: use_signal(0.4),
        hover_gloss: use_signal(false),
    };
    let rt = p.roughness.runtime();
    ui(p, rt)
}
