// Single responsibility: custom-program effect over the compositor wire
// (Phase E) — an order-reversal pair around the glass lens.
//
// Two glass islands with identical content but reversed backdrop order:
//   - "GRAIN → GLASS": Grain runs on the backdrop walk before the lens;
//     the noise travels with the sampled backdrop, so the lens refracts
//     it — grain bends/magnifies with the lens geometry.
//   - "GLASS → GRAIN": Grain runs on the finished lens card; the noise
//     sits flat over the card, pixel-regular, ignoring refraction.
//   - "GLASS ONLY" (control): no grain — if this island loses its lens
//     or both grains vanish, the pipeline broke, not the order.
//
// Grain is `EffectShader::Custom`: its module travels program →
// riode-glass plugin over `riode-item-v1` (the client prepends the
// effect ABI header), transpiles in the in-tree glue (naga → GLSL ES
// 3.00) and runs as a GL stage there. The wgpu/tiny-skia engines skip
// custom stages by design, so the demo must run as the overlay
// (`example wire_grain --overlay`): the plugin owns the backdrop path.
// The windowed preview shows layout/material only — no grain there is
// expected, not a bug.
// Islands are static on purpose: one variable per observation — the
// only difference between the two rows is stage order.

use riode::foundation::{
    BackdropEffect, BackdropNeed, CustomShader, Effect, EffectId, EffectShader, EffectStack,
    GlassEffect,
};
use riode::prelude::*;

// Presets resolve to the single shared image under hot-reload and to
// the static crate in release drivers — same values either way.
#[cfg(hot_ui)]
use riode::anodeui::GlassPreset;
#[cfg(not(hot_ui))]
use riode_anodeui::GlassPreset;

/// Window chrome shared by the release preview (`main.rs`) and the hot
/// entry (`hot.rs`): declared once here, referenced by both — never mirrored.
/// The overlay host (`apps/desktop-glass`) sizes from layer-shell configure
/// and always paints transparent, ignoring HOT_BG.
pub const HOT_TITLE: &str = "Wire grain";
pub const HOT_WIDTH: u32 = 1040;
pub const HOT_HEIGHT: u32 = 420;
pub const HOT_BG: u32 = 0x14161C;

const LABEL_COLOR: u32 = 0xFFFFFF;

/// Additive per-device-pixel hash noise, clamped to the premultiplied
/// alpha so the card stays blend-legal. Amount rides `u.data[0].x`.
/// The wire sends the ABI header + this module; the glue appends the
/// `fs_main` scaffold (bindings: 1 uniforms, 2 input, 3 backdrop, 4 sharp).
const GRAIN_WGSL: &str = r#"
fn hash21(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.xyx) * vec3<f32>(0.1031, 0.1030, 0.0973));
    p3 = p3 + dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

fn apply(
    ctx: EffectCtx,
    backdrop: texture_2d<f32>,
    sharp: texture_2d<f32>,
    u: CustomUniforms,
) -> EffectCtx {
    var out = ctx;
    let n = hash21(ctx.px) - 0.5;
    let amount = u.data[0].x;
    out.color = vec4<f32>(
        min(ctx.color.rgb + vec3<f32>(n * amount), vec3<f32>(ctx.color.a)),
        ctx.color.a,
    );
    return out;
}
"#;

/// The preset lens as one bracket element (hot and release resolve the
/// same preset values) — same shape as `effect_stack`.
fn glass() -> GlassEffect {
    GlassPreset::clear().style().into_effect()
}

/// Custom backdrop stage: grain before the lens (refracted with the
/// backdrop) or after it (flat over the card) — stack position decides,
/// nothing else about the stage changes between the two rows.
#[derive(Clone, Debug, PartialEq)]
struct Grain {
    amount: f32,
}

impl Grain {
    const fn new(amount: f32) -> Self {
        Self { amount }
    }
}

impl Effect for Grain {
    fn id(&self) -> EffectId {
        // Stable across processes (hot reload, stack equality): constant
        // tag ^ amount bits. The foundation's `hash_id` is private.
        EffectId(0x4752_4149_4E5F_7631 ^ u64::from(self.amount.to_bits()))
    }

    fn needs_backdrop(&self) -> bool {
        false
    }

    fn pad(&self, _scale: f32) -> f32 {
        // Pixel-local noise: reaches nothing past the card.
        0.0
    }

    fn shader(&self) -> EffectShader {
        EffectShader::Custom(CustomShader {
            wgsl: GRAIN_WGSL.to_string(),
            uniforms: vec![self.amount],
            backdrop: BackdropNeed::None,
        })
    }

    fn clone_box(&self) -> Box<dyn Effect> {
        Box::new(self.clone())
    }
}

// Backdrop vocabulary only: `layer_effects=[Grain::new(..)]` must not
// compile — a transparent sheet has no backdrop to grain.
impl BackdropEffect for Grain {}

#[component]
fn IslandCard(label: &'static str, material: Option<EffectStack>) -> impl IntoElement {
    let material = material.unwrap_or_default();
    view! {
        <Group direction="vertical" gap={16.0} alignment="center">
            <Text size={13.0} weight={600u16} color={LABEL_COLOR}>{label}</Text>
            <Group
                width={280.0}
                height={200.0}
                radius={24.0}
                material={material}
                alignment="center"
                distribution="center"
            >
                <Text size={44.0} weight={700u16} color={Color::WHITE}>{"Aa"}</Text>
            </Group>
        </Group>
    }
}

#[component]
pub fn Page() -> impl IntoElement {
    view! {
        <Group
            direction="horizontal"
            width="fill"
            height="fill"
            alignment="center"
            distribution="center"
            gap={56.0}
        >
            <IslandCard label="GRAIN → GLASS" material=[Grain::new(0.5), glass()] />
            <IslandCard label="GLASS → GRAIN" material=[glass(), Grain::new(0.5)] />
            <IslandCard label="GLASS ONLY" material=[glass()] />
        </Group>
    }
}
