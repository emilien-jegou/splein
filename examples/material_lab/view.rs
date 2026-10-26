// Material lab: live instrument for the custom-stage mechanism (G0) and
// the macro-time DSL (G1). Layout mirrors `glass_studio`: a scrollable
// control sidebar left, one big editable card right.
//
// The single card has three modes, cycled by the top button:
//   - "host"   : the bare `Glass::geometry()` host alone — silhouette,
//     coverage, merge-under-sheet, zero shading.
//   - "copy"   : same host + a hand-written passthrough surface. Must
//     render IDENTICALLY to "host" — this falsifies host neutrality
//     (D-M4). If it differs in anything, the negative result reopens the
//     pre-glass stage position, not the scaffold.
//   - "live"   : the macro-time DSL surface from the sidebar sliders.
//     Dragging a slider refills one uniform slot; the generated WGSL
//     source never changes (pipeline cache must hit — D-M5).
//
// Per AGENTS.md: predictions live below for the human to check, not
// claims. Drag every slider end to end: if any slider changes the card's
// *shape* (rounded corners, silhouette) instead of its *shading*, the
// structure/value split is leaking.

use riode::foundation::{BackdropNeed, CustomEffect, EffectStack};
use riode::foundation::Glass;
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

const MODE_NAMES: [&str; 3] = ["host", "copy", "live"];
const MODE_DESCRIPTIONS: [&str; 3] = [
    "bare host: silhouette + coverage, zero shading",
    "host + hand-written passthrough (must equal host)",
    "host + DSL surface from the sliders below",
];

/// Identity surface: copies the card through untouched. The G0 proof —
/// "copy" must equal "host" pixel-for-pixel (compare with `s` captures
/// under F8 freeze). Reads nothing but `ctx.color` (the card the host
/// built).
const PASSTHROUGH_WGSL: &str = r#"
fn apply(
    ctx: EffectCtx,
    backdrop: texture_2d<f32>,
    sharp: texture_2d<f32>,
    u: CustomUniforms,
) -> EffectCtx {
    var out = ctx;
    out.color = ctx.color;
    return out;
}
"#;

fn surface(wgsl: &'static str, uniforms: Vec<f32>) -> EffectStack {
    EffectStack(vec![
        CustomEffect::new(wgsl, uniforms, BackdropNeed::None).into_effect(),
    ])
}

/// All live state, one signal per control. `mode` picks which stack the
/// card wears (0 = host, 1 = copy, 2 = live DSL). `bg` is the stage
/// lightness so shading reads on light and dark grounds alike.
#[derive(Clone)]
struct Params {
    mode: Signal<i32>,
    bg: Signal<f32>,
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
    // Stage ground follows the `bg` slider: the signal maps straight to
    // a `Color`, and `fill` accepts `Signal<Color>` (`dsl/prop.rs`) —
    // no `computed` wrapper, no `Prop<Color>` mismatch.
    let stage_fill = p.bg.map(|v| {
        let v = v.clamp(0.0, 1.0);
        Color::rgb(v, v, v)
    });
    // Readout values through one `computed` (folded, so the panel keeps
    // the sliders' width as it lengthens).
    let readout = {
        let p = p.clone();
        computed(move || {
            let idx = p.mode.get().clamp(0, 2) as usize;
            format!(
                "mode {} · bg {:.2} · rough {:.2} · tint ({:.2}, {:.2}, {:.2}) · gain {:.2} · gloss {:.2}{}",
                MODE_NAMES[idx],
                p.bg.get(),
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
    let mode_about = {
        let m = p.mode;
        computed(move || MODE_DESCRIPTIONS[m.get().clamp(0, 2) as usize].to_string())
    };
    let f2: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.2}"));
    let mode = p.mode;
    let is_host = mode.map(|m| m == 0);
    let is_copy = mode.map(|m| m == 1);
    let is_live = mode.map(|m| m != 0 && m != 1);
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
                        let mode = p.mode;
                        computed(move || {
                            let idx = mode.get().clamp(0, 2) as usize;
                            format!("card: {} (tap: next)", MODE_NAMES[idx])
                        })
                    },
                    {
                        let p = p.clone();
                        move || p.mode.set((p.mode.get() + 1).rem_euclid(3))
                    },
                )}
                {mode_about}
                {slider(&rt, "background", "stage lightness: slide to read the sheen on dark and light grounds.", p.bg, 0.0, 1.0, f2.clone())}
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
            // Preview stage: one big editable card on the sliding ground.
            // Each mode is its own branch with its stack INLINE in the
            // `backdrop_effects` attribute — the macro only intercepts
            // material blocks there. Host/copy wear ready-built stacks;
            // the live branch's block reads slider signals as plain
            // numerics (value-level), refilling one uniform slot per
            // drag with byte-identical WGSL source (cache hit — D-M5).
            <Group
                width="fill"
                height="fill"
                fill={stage_fill}
                clip={true}
                alignment="center"
                distribution="center"
            >
                <Group direction="vertical" gap={12.0} alignment="center">
                    <if is_host.get()>
                        <Group
                            width={420.0}
                            height={330.0}
                            radius={24.0}
                            backdrop_effects={EffectStack(vec![Glass::geometry().into_effect()])}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={72.0} weight={700u16} color={Color::hex(0xC81E3A)}>{"Aa"}</Text>
                        </Group>
                    </if>
                    <if is_copy.get()>
                        <Group
                            width={420.0}
                            height={330.0}
                            radius={24.0}
                            backdrop_effects={surface(PASSTHROUGH_WGSL, vec![])}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={72.0} weight={700u16} color={Color::hex(0xC81E3A)}>{"Aa"}</Text>
                        </Group>
                    </if>
                    <if is_live.get()>
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
                            <Text size={72.0} weight={700u16} color={Color::hex(0xC81E3A)}>{"Aa"}</Text>
                        </Group>
                    </if>
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
        mode: use_signal(2i32),
        bg: use_signal(0.16),
        roughness: use_signal(0.35),
        tint_r: use_signal(1.0),
        tint_g: use_signal(0.25),
        tint_b: use_signal(0.2),
        gain: use_signal(1.0),
        gloss: use_signal(0.25),
        hover_gloss: use_signal(false),
    };
    let rt = p.roughness.runtime();
    ui(p, rt)
}
