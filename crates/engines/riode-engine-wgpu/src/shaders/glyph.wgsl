// Glyph coverage sampling: plain alpha coverage from the R8 atlas,
// corrected for display, then * vertex alpha.
//
// Stores the exact coverage bitmap from the analytic rasterizer.
// No MSDF decoding needed.
//
// Display correction is the grayscale contrast + gamma chain ported
// from GPUI's shaders.wgsl (itself adapted from microsoft/terminal
// dwrite.hlsl, MIT), applied at ALL sizes: brightness-dependent
// contrast boost plus a quadratic alpha correction driven by the
// baked gamma-1.8 ratio row. There is deliberately no font-size
// threshold — the chain is coverage-proportional, so dense stems pass
// through nearly unchanged while faint fringes lift.
// Scalar twin of the correction lives in riode's text::gamma
// (tiny-skia builds its per-run table from it); this WGSL is a
// transliteration of that module. Selected only with
// RIODE_TEXT_CORRECTION=on (see pipeline.rs); default is the
// passthrough shader glyph_plain.wgsl.
struct Ortho {
    w: f32,
    h: f32,
};
@group(0) @binding(0)
var<uniform> ortho: Ortho;
@group(1) @binding(0)
var atlas: texture_2d<f32>;
@group(1) @binding(1)
var atlas_sampler: sampler;

struct Vin {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: Vin) -> Vout {
    var out: Vout;
    let nx = (in.pos.x / ortho.w) * 2.0 - 1.0;
    let ny = 1.0 - (in.pos.y / ortho.h) * 2.0;
    out.pos = vec4<f32>(nx, ny, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

// Grayscale contrast + gamma correction, verbatim port of GPUI's
// shaders.wgsl chain (itself adapted from microsoft/terminal
// dwrite.hlsl, MIT). Opt-in via RIODE_TEXT_CORRECTION=on; default is
// the passthrough shader (glyph_plain.wgsl).
//
// Defaults match GPUI: gamma 1.8 ratio row baked below,
// grayscale enhanced contrast 1.0 (override via uniform later).
const GAMMA_RATIOS = vec4<f32>(0.14805442, -0.89459451, 1.47590801, -0.32466824);
const GRAYSCALE_ENHANCED_CONTRAST = 1.0;

fn color_brightness(color: vec3<f32>) -> f32 {
    return dot(color, vec3<f32>(0.30, 0.59, 0.11));
}

fn light_on_dark_contrast(enhanced_contrast: f32, color: vec3<f32>) -> f32 {
    let brightness = color_brightness(color);
    let multiplier = saturate(4.0 * (0.75 - brightness));
    return enhanced_contrast * multiplier;
}

fn enhance_contrast(alpha: f32, k: f32) -> f32 {
    return alpha * (k + 1.0) / (alpha * k + 1.0);
}

fn apply_alpha_correction(a: f32, b: f32, g: vec4<f32>) -> f32 {
    let brightness_adjustment = g.x * b + g.y;
    let correction = brightness_adjustment * a + (g.z * b + g.w);
    return a + a * (1.0 - a) * correction;
}

fn apply_contrast_and_gamma_correction(
    sample: f32,
    color: vec3<f32>,
    enhanced_contrast_factor: f32,
    gamma_ratios: vec4<f32>,
) -> f32 {
    let enhanced_contrast = light_on_dark_contrast(enhanced_contrast_factor, color);
    let brightness = color_brightness(color);
    let contrasted = enhance_contrast(sample, enhanced_contrast);
    return apply_alpha_correction(contrasted, brightness, gamma_ratios);
}

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    let sample = textureSample(atlas, atlas_sampler, in.uv).r;
    let cov = apply_contrast_and_gamma_correction(
        sample,
        in.color.rgb,
        GRAYSCALE_ENHANCED_CONTRAST,
        GAMMA_RATIOS,
    );
    // Encoded-space composite on the plain Unorm target (vello-like).
    return vec4<f32>(in.color.rgb, in.color.a * cov);
}
