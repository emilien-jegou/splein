// Single responsibility: Direct A/B side-by-side comparison.
//
// LEFT  = Verbatim line-for-line port of the engine's glass/ shader files.
// RIGHT = Target DSL implementation for liquid_glass_new.

#![allow(deprecated)]

use riode::foundation::{BackdropNeed, Color, Fill, Shadow, Surface};
use riode::internal::group;
use riode::prelude::*;

const BG: u32 = 0xFFFFFF;
pub const HOT_TITLE: &str = "Liquid Glass — Left (Engine Verbatim WGSL) vs. Right (Target DSL)";
pub const HOT_WIDTH: u32 = 1100;
pub const HOT_HEIGHT: u32 = 640;
pub const HOT_BG: u32 = BG;

const LABEL_COLOR: u32 = 0x333A44;
const INK_COLOR: u32 = 0x1C2333;

fn preset_shadow() -> Shadow {
    Shadow::layered(Color::BLACK).ease_out(10.0, 20.0, 0.04)
}

// ============================================================================
// 1. LEFT CARD: Real Engine Pipeline Ported Verbatim from `shaders/glass/`
// ============================================================================
//
// Target API:
//
//   GlassPreset::clear().style().into_effects()
//
// Verbatim modules transcribed:
//   - shaders/glass/shape.wgsl      (sdSuperellipseBox, shapeOf, stageShape)
//   - shaders/glass/refract.wgsl    (stageRefract with lensK, cut, v, le)
//   - shaders/glass/frost.wgsl      (blur_sigma: 1.28)
//   - shaders/glass/saturation.wgsl (stageSaturation: 0.9726)
//   - shaders/glass/tone.wgsl       (toneLut with CLEAR_TONE knots)
//   - shaders/glass/wash.wgsl       (stageWash: fill 0xFEFCFC, opacity 0.1705)
//   - shaders/glass/rim.wgsl        (stageRim: rimW 1.2475, rimI 0.15)
//
// ============================================================================
const LEFT_VERBATIM_ENGINE_WGSL: &str = r#"
// --- from shaders/glass/shape.wgsl ---
fn sdSuperellipseBox(p: vec2<f32>, halfSize: vec2<f32>, r0: f32, n0: f32) -> f32 {
    let r = min(r0, min(halfSize.x, halfSize.y));
    let q = abs(p) - halfSize + r;
    let m = max(q, vec2<f32>(0.0));
    let corner = pow(pow(m.x, n0) + pow(m.y, n0), 1.0 / n0);
    return corner + min(max(q.x, q.y), 0.0) - r;
}

fn shapeOf(p: vec2<f32>, rc: vec4<f32>, info: vec4<f32>, rs: f32, zf: f32) -> f32 {
    let hs = rc.zw * 0.5;
    let hm = min(hs.x, hs.y);
    var r = min(info.x * rs, hm);
    var n = 2.0; // standard circular arc
    return sdSuperellipseBox(p - (rc.xy + hs), hs, r, n);
}

// --- from shaders/glass/tone.wgsl ---
fn toneLut(c: vec3<f32>, a: vec4<f32>, b: vec4<f32>, k8: f32) -> vec3<f32> {
    let c8 = c * 8.0;
    return vec3<f32>(a.x)
        + (vec3<f32>(a.y) - vec3<f32>(a.x)) * clamp(c8, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(a.z) - vec3<f32>(a.y)) * clamp(c8 - 1.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(a.w) - vec3<f32>(a.z)) * clamp(c8 - 2.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.x) - vec3<f32>(a.w)) * clamp(c8 - 3.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.y) - vec3<f32>(b.x)) * clamp(c8 - 4.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.z) - vec3<f32>(b.y)) * clamp(c8 - 5.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.w) - vec3<f32>(b.z)) * clamp(c8 - 6.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(k8) - vec3<f32>(b.w)) * clamp(c8 - 7.0, vec3<f32>(0.0), vec3<f32>(1.0));
}

// --- 1.28px blur tap kernel (shaders/glass/frost.wgsl) ---
fn engine_sample_blur(sharp: texture_2d<f32>, sample_point: vec2<f32>) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(sharp));
    let max_d = max(dims, vec2<f32>(1.0));
    let r = 1.28;
    
    let c0 = textureSampleLevel(sharp, effect_sampler, clamp(sample_point / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let c1 = textureSampleLevel(sharp, effect_sampler, clamp((sample_point + vec2<f32>(r, 0.0)) / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let c2 = textureSampleLevel(sharp, effect_sampler, clamp((sample_point - vec2<f32>(r, 0.0)) / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let c3 = textureSampleLevel(sharp, effect_sampler, clamp((sample_point + vec2<f32>(0.0, r)) / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let c4 = textureSampleLevel(sharp, effect_sampler, clamp((sample_point - vec2<f32>(0.0, r)) / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    
    let avg = c0.rgb * 0.40 + (c1.rgb + c2.rgb + c3.rgb + c4.rgb) * 0.15;
    let a = max(c0.a, 1.0 / 255.0);
    return avg / a;
}

fn apply(
    ctx: EffectCtx,
    backdrop: texture_2d<f32>,
    sharp: texture_2d<f32>,
    u: CustomUniforms,
) -> EffectCtx {
    var out = ctx;
    let card_a = ctx.color.a;

    // Card geometry parameters
    let rc = vec4<f32>(0.0, 0.0, 320.0, 200.0);
    let info = vec4<f32>(24.0, 0.0, 0.0, 1.0); // radius 24.0
    let px = ctx_local_px(ctx);

    // 1. stageShape (shaders/glass/shape.wgsl): normal from lensField with normal_radius_scale = 1.55
    let rs = 1.55;
    let d = shapeOf(px, rc, info, 1.0, 1.0);
    let depth = -d; // positive inside, negative outside

    let fxp = shapeOf(px + vec2<f32>(1.0, 0.0), rc, info, rs, 1.0);
    let fxm = shapeOf(px - vec2<f32>(1.0, 0.0), rc, info, rs, 1.0);
    let fyp = shapeOf(px + vec2<f32>(0.0, 1.0), rc, info, rs, 1.0);
    let fym = shapeOf(px - vec2<f32>(0.0, 1.0), rc, info, rs, 1.0);
    let grad = vec2<f32>(fxp - fxm, fyp - fym) + vec2<f32>(1e-6);
    let gradLen = length(grad);
    let normal = grad / gradLen; // outward unit normal

    // 2. stageRefract (shaders/glass/refract.wgsl verbatim):
    let band = 18.442;
    let decay = 6.8073;
    let lens_strength = -2.49;
    let lens_edge = 10.0;
    let lens_edge_decay = 0.6;

    let ldepth = depth;
    let cut = exp(-band / decay);
    let ex = exp(-max(ldepth, 0.0) / decay);
    // Smooth cutoff: v is strictly 0.0 when depth >= band!
    let v = max(ex - cut, 0.0) / max(1.0 - cut, 1e-6);
    let lensK = lens_strength * band; // -2.49 * 18.442 = -45.92px
    var lensAmt = lensK * v;
    let le = lens_edge * exp(-max(ldepth, 0.0) / lens_edge_decay);
    lensAmt -= le;

    // Displacement along normal: normal is outward, lensAmt is negative -> pulls inward!
    let sample_point = ctx.px + normal * lensAmt;

    // 3. stageFrost (shaders/glass/frost.wgsl)
    var col = engine_sample_blur(sharp, sample_point);

    // 4. stageSaturation (shaders/glass/saturation.wgsl): saturation = 0.9726
    let luma = dot(col, vec3<f32>(0.2126, 0.7152, 0.0722));
    col = mix(vec3<f32>(luma), col, 0.9726);

    // 5. stageToneCurve (shaders/glass/tone.wgsl): CLEAR_TONE knots
    let G = vec4<f32>(0.0, 0.0289, 0.1865, 0.3476);
    let H = vec4<f32>(0.5063, 0.6651, 0.8265, 0.9772);
    let Ix = 1.0;
    col = toneLut(col, G, H, Ix);

    // 6. stageWash (shaders/glass/wash.wgsl): fill_color = 0xFEFCFC, fill_opacity = 0.1705
    let fill_color = vec3<f32>(0.99608, 0.98824, 0.98824);
    col = mix(col, fill_color, 0.1705);

    // 7. stageRim (shaders/glass/rim.wgsl): rim_width = 1.2475, rim_intensity = 0.15
    let rimW = 1.2475;
    let rim = 1.0 - smoothstep(0.0, rimW, depth);
    col += vec3<f32>(0.15 * rim);

    out.color = vec4<f32>(col * card_a, card_a);
    return out;
}
"#;

// ============================================================================
// 2. RIGHT CARD: Target DSL Representation
// ============================================================================
//
// Target API:
//
//   let n = Lens::beveled(-2.49, 18.442);
//   Transmit::sample()
//       .normal(n)
//       .ior(1.50)
//       .frost(1.28)
//       .build()
//       .into_surface()
//
// ============================================================================
const RIGHT_TARGET_DSL_WGSL: &str = r#"
fn dsl_sd_rounded_box(p: vec2<f32>, size: vec2<f32>, r: f32) -> f32 {
    let half = size * 0.5;
    let rad = clamp(r, 0.0, min(half.x, half.y));
    let q = abs(p - half) - half + vec2<f32>(rad);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - rad;
    return max(-d, 0.0);
}

fn dsl_sample_polished(sharp: texture_2d<f32>, refr: vec2<f32>) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(sharp));
    let max_d = max(dims, vec2<f32>(1.0));
    let r = 1.28;
    
    let c0 = textureSampleLevel(sharp, effect_sampler, clamp(refr / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let c1 = textureSampleLevel(sharp, effect_sampler, clamp((refr + vec2<f32>(r, 0.0)) / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let c2 = textureSampleLevel(sharp, effect_sampler, clamp((refr - vec2<f32>(r, 0.0)) / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let c3 = textureSampleLevel(sharp, effect_sampler, clamp((refr + vec2<f32>(0.0, r)) / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let c4 = textureSampleLevel(sharp, effect_sampler, clamp((refr - vec2<f32>(0.0, r)) / max_d, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    
    let avg = c0.rgb * 0.40 + (c1.rgb + c2.rgb + c3.rgb + c4.rgb) * 0.15;
    let a = max(c0.a, 1.0 / 255.0);
    return avg / a;
}

fn apply(
    ctx: EffectCtx,
    backdrop: texture_2d<f32>,
    sharp: texture_2d<f32>,
    u: CustomUniforms,
) -> EffectCtx {
    var out = ctx;
    let card_a = ctx.color.a;

    let card_size = vec2<f32>(320.0, 200.0);
    let corner_r = 24.0;
    let px = ctx_local_px(ctx);
    let depth = dsl_sd_rounded_box(px, card_size, corner_r);

    let dx1 = dsl_sd_rounded_box(px + vec2<f32>(1.55, 0.0), card_size, corner_r);
    let dx0 = dsl_sd_rounded_box(px - vec2<f32>(1.55, 0.0), card_size, corner_r);
    let dy1 = dsl_sd_rounded_box(px + vec2<f32>(0.0, 1.55), card_size, corner_r);
    let dy0 = dsl_sd_rounded_box(px - vec2<f32>(0.0, 1.55), card_size, corner_r);
    let grad = vec2<f32>((dx1 - dx0) * 0.5, (dy1 - dy0) * 0.5);
    let grad_len = max(length(grad), 1e-4);
    let normal = grad / grad_len;

    // Method 2 exact windowed lens formula
    let band = 18.442;
    let decay = 6.8073;
    let cut = exp(-band / decay);
    let ex = exp(-max(depth, 0.0) / decay);
    let v = max(ex - cut, 0.0) / max(1.0 - cut, 1e-6);
    
    let lens_strength = -2.49;
    var lensAmt = (lens_strength * band) * v;
    let le = 10.0 * exp(-max(depth, 0.0) / 0.6);
    lensAmt -= le;

    // Displace along normal
    let refr = ctx.px - normal * lensAmt;
    var col = dsl_sample_polished(sharp, refr);

    let rim = 1.0 - smoothstep(0.0, 1.25, depth);
    col += vec3<f32>(1.0, 1.0, 1.0) * (0.15 * rim);

    out.color = vec4<f32>(col * card_a, card_a);
    return out;
}
"#;

// ============================================================================
// PAGE VIEW COMPONENT
// ============================================================================
#[component]
pub fn Page() -> impl IntoElement {
    let clock = use_frame_clock();
    let t = clock.map(|secs| {
        let phase = (secs % 8.0) / 8.0;
        let tri = if phase < 0.5 {
            phase * 2.0
        } else {
            2.0 - phase * 2.0
        };
        tri as f32
    });

    view! {
        <Group
            direction="vertical"
            width="fill"
            height="fill"
            fill={Fill::Solid(Color::hex(HOT_BG))}
            clip={true}
            alignment="center"
            distribution="center"
            gap={40.0}
        >
            // Background circles sweeping directly across the card bevels
            {blob(
                t.clone(),
                160.0,
                Color::hex_alpha(0x3B82F6, 0.90),
                |v| Transform::from_translation(v * 760.0 + 80.0, 185.0), // Sweeps top bevels
            )}
            {blob(
                t.clone(),
                130.0,
                Color::hex_alpha(0xE5484D, 0.85),
                |v| Transform::from_translation(860.0 - v * 760.0, 275.0), // Sweeps bottom bevels
            )}
            {blob(
                t,
                110.0,
                Color::hex_alpha(0x30A46C, 0.85),
                |v| Transform::from_translation(v * 500.0 + 300.0, 230.0), // Sweeps center
            )}

            <Group
                direction="vertical"
                gap={36.0}
                alignment="center"
                distribution="center"
                z_index={2}
            >
                <Group direction="horizontal" gap={56.0} alignment="center">
                    // ========================================================
                    // LEFT CARD: Verbatim Engine Pipeline
                    // ========================================================
                    <Group direction="vertical" gap={16.0} alignment="center">
                        <Group
                            width={320.0}
                            height={200.0}
                            radius={24.0}
                            material={Surface {
                                wgsl: LEFT_VERBATIM_ENGINE_WGSL.to_string(),
                                uniforms: vec![],
                                backdrop: BackdropNeed::ReadSharp,
                                pad: Some(64.0),
                            }}
                            shadow={preset_shadow()}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={15.0} weight={700u16} color={Color::hex(INK_COLOR)}>
                                {"LEFT: Verbatim Engine WGSL"}
                            </Text>
                        </Group>
                        <Text size={12.0} weight={600u16} color={LABEL_COLOR}>
                            {"Ported directly from glass/*.wgsl"}
                        </Text>
                    </Group>

                    // ========================================================
                    // RIGHT CARD: Target DSL Representation
                    // ========================================================
                    <Group direction="vertical" gap={16.0} alignment="center">
                        <Group
                            width={320.0}
                            height={200.0}
                            radius={24.0}
                            material={Surface {
                                wgsl: RIGHT_TARGET_DSL_WGSL.to_string(),
                                uniforms: vec![],
                                backdrop: BackdropNeed::ReadSharp,
                                pad: Some(64.0),
                            }}
                            shadow={preset_shadow()}
                            alignment="center"
                            distribution="center"
                        >
                            <Text size={15.0} weight={700u16} color={Color::hex(INK_COLOR)}>
                                {"RIGHT: Target DSL WGSL"}
                            </Text>
                        </Group>
                        <Text size={12.0} weight={600u16} color={LABEL_COLOR}>
                            {"Clean DSL-targeted liquid refraction WGSL"}
                        </Text>
                    </Group>
                </Group>
            </Group>
        </Group>
    }
}

fn blob(
    t: Computed<f32>,
    size: f32,
    fill: Color,
    place: impl Fn(f32) -> Transform + 'static,
) -> Element {
    group()
        .width(size)
        .height(size)
        .radius("max")
        .absolute()
        .fill(fill)
        .transform(t.map(move |v| place(v)))
        .into_element()
}
