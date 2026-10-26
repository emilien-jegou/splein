// Material-lab demo (Phase 0, gate G0): the custom-stage mechanism proof.
//
// Three rows, one variable apart — the human compares them by eye:
//   - "OFF" (control): a neutral `Glass::geometry()` host alone. The
//     bare card: silhouette, coverage, merge-under-sheet, zero shading.
//   - "ON" (custom surface): the same host + a hand-written passthrough
//     surface (`CustomEffect` carrying the `PASSTHROUGH_WGSL` module).
//     Must render IDENTICALLY to OFF — this falsifies host neutrality
//     (D-M4). Any tint, edge, or brightness delta means the host shades
//     and the DSL would inherit the lie.
//   - "TINT" (custom shades): the same host + `TINT_WGSL`, which adds a
//     uniform-driven red lift. Proves the stage actually executes and
//     the uniform path works (the value `0.15` rides uniforms).
//
// Per AGENTS.md: these predictions are for the human to check, not
// claims. If ON differs from OFF in anything but TINT's red lift, the
// negative result reopens the pre-glass stage position or the missing
// composite path — not the scaffold.

use riode::foundation::{
    BackdropNeed, CustomEffect, EffectStack,
};
use riode::foundation::Glass;
use riode::prelude::*;

/// Window chrome shared by the release driver (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both.
pub const HOT_TITLE: &str = "Material lab (Phase 0)";
pub const HOT_WIDTH: u32 = 1100;
pub const HOT_HEIGHT: u32 = 520;
pub const HOT_BG: u32 = 0x2A3F55;

const LABEL_COLOR: u32 = 0xFFFFFF;
const INK_COLOR: u32 = 0xC81E3A;

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

/// Backdrop food: a saturated gradient field so refraction/host edges
/// would show. Y-invariant stripes keep rows comparable.
fn stripes() -> Vec<Element> {
    const W: f32 = 67.0;
    const COLORS: [u32; 4] = [0x3A2424, 0x3D3416, 0x1E3329, 0x1E2A45];
    let mut out = Vec::new();
    for i in 0..20 {
        out.push(
            riode::internal::group()
                .width(W)
                .height(520.0)
                .absolute()
                .fill(Color::hex(COLORS[i % 4]))
                .transform(Transform::from_translation(i as f32 * W, 0.0))
                .into_element(),
        );
    }
    out
}

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            fill={Color::hex(HOT_BG)}
            alignment="center"
            distribution="center"
        >
            {stripes()}
            <Group direction="vertical" width="fill" gap={24.0} alignment="center" distribution="center" z_index={2}>
                <Group direction="horizontal" gap={48.0} alignment="center">
                    <Card
                        label="OFF (host only)"
                        backdrop_effects={EffectStack(vec![Glass::geometry().into_effect()])}
                    />
                    <Card
                        label="ON (passthrough)"
                        backdrop_effects={surface(PASSTHROUGH_WGSL, vec![])}
                    />
                    <Card
                        label="TINT (red lift)"
                        backdrop_effects={surface(TINT_WGSL, vec![0.15])}
                    />
                </Group>
            </Group>
        </Group>
    }
}
