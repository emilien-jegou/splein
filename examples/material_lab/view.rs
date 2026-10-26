// Material lab: live instrument for the custom-stage mechanism (G0) and
// the macro-time DSL (G1).
//
// Left: reference rows, one variable apart — the human compares by eye:
//   - "OFF" (control): a neutral `Glass::geometry()` host alone. The
//     bare card: silhouette, coverage, merge-under-sheet, zero shading.
//   - "ON" (custom surface): the same host + a hand-written passthrough
//     surface. Must render IDENTICALLY to OFF — this falsifies host
//     neutrality (D-M4).
//   - "DSL" (macro surface): the same look as TINT/plaque but written in
//     DSL syntax — must render identically to its raw-WGSL twin.
//
// Right: the live DSL card. Every numeric value position in the material
// block is bound to a slider signal; dragging a slider refills that one
// uniform slot (same WGSL source — the pipeline cache must hit). The
// readout line shows the actual filler values each frame, and the hover
// toggle flips the `gloss` slot between its two states without touching
// structure.
//
// Per AGENTS.md: predictions live below for the human to check, not
// claims. Drag every slider end to end: if any slider changes the card's
// *shape* (rounded corners, silhouette) instead of its *shading*, the
// structure/value split is leaking. If ON differs from OFF in anything,
// the negative result reopens the pre-glass stage position or the
// missing composite path — not the scaffold.

use riode::foundation::{BackdropNeed, CustomEffect, EffectStack};
use riode::foundation::Glass;
use riode::prelude::*;
use riode::reactive::{ReactiveRuntime, computed};
use std::cell::RefCell;
use std::rc::Rc;

/// Window chrome shared by the release driver (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both.
pub const HOT_TITLE: &str = "Material lab";
pub const HOT_WIDTH: u32 = 1480;
pub const HOT_HEIGHT: u32 = 720;
pub const HOT_BG: u32 = 0x2A3F55;

const LABEL_COLOR: u32 = 0xFFFFFF;
const INK_COLOR: u32 = 0xC81E3A;

const TRACK_W: f32 = 200.0;
const TRACK_H: f32 = 28.0;
const THUMB_W: f32 = 14.0;

/// Identity surface: copies the card through untouched. The G0 proof —
/// ON must equal OFF pixel-for-pixel (compare with `s` captures under
/// F8 freeze). Reads nothing but `ctx.color` (the card the host built).
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

/// Tint surface: the same passthrough plus a uniform-driven red lift
/// (`u.data[0].x`). Proves the stage executes and uniforms arrive.
const TINT_WGSL: &str = r#"
fn apply(
    ctx: EffectCtx,
    backdrop: texture_2d<f32>,
    sharp: texture_2d<f32>,
    u: CustomUniforms,
) -> EffectCtx {
    var out = ctx;
    let lift = u.data[0].x;
    out.color = vec4<f32>(
        min(ctx.color.rgb + vec3<f32>(lift, 0.0, 0.0), vec3<f32>(ctx.color.a)),
        ctx.color.a,
    );
    return out;
}
"#;

/// A material card: `backdrop_effects` drives the whole look. The host is
/// auto-stamped by `hierarchy.rs` whenever a Custom-bearing stack has no
/// authored glass (D-M4) — so these stacks spell only the surface.
#[component]
fn Card(label: &'static str, backdrop_effects: Option<EffectStack>) -> impl IntoElement {
    let backdrop_effects = backdrop_effects.unwrap_or_default();
    view! {
        <Group direction="vertical" gap={8.0} alignment="center">
            <Text size={13.0} weight={600u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={280.0}
                height={220.0}
                radius={24.0}
                backdrop_effects={backdrop_effects}
                alignment="center"
                distribution="center"
            >
                <Text size={48.0} weight={700u16} color={Color::hex(INK_COLOR)}>{"Aa"}</Text>
            </Group>
        </Group>
    }
}

fn surface(wgsl: &'static str, uniforms: Vec<f32>) -> EffectStack {
    EffectStack(vec![
        CustomEffect::new(wgsl, uniforms, BackdropNeed::None).into_effect(),
    ])
}

/// All live DSL values: one signal per numeric position in the material
/// block. Dragging a slider refills one uniform slot; the generated WGSL
/// source never changes (pipeline cache must hit — D-M5).
#[derive(Clone)]
struct Params {
    roughness: Signal<f32>,
    tint_r: Signal<f32>,
    tint_g: Signal<f32>,
    tint_b: Signal<f32>,
    gain: Signal<f32>,
    gloss: Signal<f32>,
    hover_gloss: Signal<bool>,
}

/// A labeled slider bound to `value` over `[min, max]`.
///
/// Relative dragging (glass_studio pattern): the gesture anchors on grab,
/// then maps horizontal travel to the range, so no geometry reads needed.
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
                <Text size={13.0} color={0x9AA0AE}>{label}</Text>
                <Text size={13.0} color={0xFFFFFF}>{readout}</Text>
            </Group>
            <Group
                width={TRACK_W}
                height={TRACK_H}
                radius={6.0}
                fill={0x1E2230}
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

/// A small labeled toggle button.
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

/// Backdrop food: a saturated gradient field so refraction/host edges
/// would show. Y-invariant stripes keep rows comparable.
fn stripes() -> Vec<Element> {
    const W: f32 = 74.0;
    const COLORS: [u32; 4] = [0x3A2424, 0x3D3416, 0x1E3329, 0x1E2A45];
    let mut out = Vec::new();
    for i in 0..20 {
        out.push(
            riode::internal::group()
                .width(W)
                .height(720.0)
                .absolute()
                .fill(Color::hex(COLORS[i % 4]))
                .transform(Transform::from_translation(i as f32 * W, 0.0))
                .into_element(),
        );
    }
    out
}

fn ui(p: Params, rt: Rc<RefCell<ReactiveRuntime>>) -> Element {
    // The live DSL card: every value position reads its slider signal.
    // All reads are plain numerics (value-level) — the tree shape is
    // fixed, the filler refills slots per frame (D-M6). The Dynamic prop
    // (slots > 0) re-evaluates under the paint subscriber, so each drag
    // subscribes and repaints with new uniform *values*, byte-identical
    // WGSL source (pipeline cache hit per D-M5).
    let f2: Rc<dyn Fn(f32) -> String> = Rc::new(|v| format!("{v:.2}"));
    let readout = {
        let p = p.clone();
        computed(move || {
            format!(
                "rough {:.2} · tint ({:.2}, {:.2}, {:.2}) · gain {:.2} · gloss {:.2}{}",
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
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height="fill"
            fill={Color::hex(HOT_BG)}
            alignment="center"
        >
            {stripes()}
            // Reference rows: G0 identity proof + DSL/raw-WGSL twins.
            <Group direction="vertical" gap={16.0} alignment="center" distribution="center" z_index={2}>
                <Group direction="horizontal" gap={32.0} alignment="center">
                    <Card
                        label="OFF (host only)"
                        backdrop_effects={EffectStack(vec![Glass::geometry().into_effect()])}
                    />
                    <Card
                        label="ON (passthrough)"
                        backdrop_effects={surface(PASSTHROUGH_WGSL, vec![])}
                    />
                    <Card
                        label="DSL (plaque)"
                        backdrop_effects={
                            let n = Normal::flat();
                            let lobe = Microfacet::ggx(n, 0.35);
                            let tinted = Color::rgb(1.0, 0.25, 0.2) * lobe;
                            (tinted * Sky::gain(1.0)).into_surface()
                        }
                    />
                </Group>
                <Text size={12.0} color={0x9AA0AE}>{"OFF vs ON: identical (host neutral). DSL plaque: opaque reddish sheen."}</Text>
            </Group>
            // Live instrument: sliders left, DSL card right.
            <Group direction="horizontal" gap={24.0} alignment="center" z_index={2}>
                <ScrollView width={248.0} height="fill">
                <Group direction="vertical" gap={10.0} margin={24.0}>
                    {slider(&rt, "roughness", p.roughness, 0.02, 1.0, f2.clone())}
                    {slider(&rt, "tint_r", p.tint_r, 0.0, 1.0, f2.clone())}
                    {slider(&rt, "tint_g", p.tint_g, 0.0, 1.0, f2.clone())}
                    {slider(&rt, "tint_b", p.tint_b, 0.0, 1.0, f2.clone())}
                    {slider(&rt, "env gain", p.gain, 0.0, 2.0, f2.clone())}
                    {slider(&rt, "gloss", p.gloss, 0.0, 1.0, f2.clone())}
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
                    <Text size={12.0} color={0x9AA0AE} width="fold">{readout}</Text>
                </Group>
                </ScrollView>
                <Group direction="vertical" gap={8.0} alignment="center">
                    <Text size={13.0} weight={600u16} color={LABEL_COLOR}>{"LIVE (drag sliders)"}</Text>
                    <Group
                        width={280.0}
                        height={220.0}
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
                        <Text size={48.0} weight={700u16} color={Color::hex(INK_COLOR)}>{"Aa"}</Text>
                    </Group>
                </Group>
            </Group>
        </Group>
    }
    .into_element()
}

#[component]
pub fn Page() -> impl IntoElement {
    let p = Params {
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
