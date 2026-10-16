// Glyph coverage sampling: plain alpha coverage from the R8 atlas,
// then through the display transfer function, then * vertex alpha.
//
// Stores the exact coverage bitmap from the analytic rasterizer.
// No MSDF decoding needed -- just multiply alpha by the sampled value.
//
// Transfer matches tiny-skia's GAMMA_LUT (COVERAGE_GAMMA=2.2):
// small hinted text (<=16px, gam flag 0) composites raw coverage;
// display type (gam flag 1) remaps coverage so stem weights match.
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
    @location(3) gam: f32,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) gam: f32,
};

@vertex
fn vs_main(in: Vin) -> Vout {
    var out: Vout;
    let nx = (in.pos.x / ortho.w) * 2.0 - 1.0;
    let ny = 1.0 - (in.pos.y / ortho.h) * 2.0;
    out.pos = vec4<f32>(nx, ny, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    out.gam = in.gam;
    return out;
}

fn gamma_lut(cov: f32) -> f32 {
    return 1.0 - pow(1.0 - cov, 1.0 / 2.2);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    var cov = textureSample(atlas, atlas_sampler, in.uv).r;
    if (in.gam > 0.5) {
        cov = gamma_lut(cov);
    }
    // Vertex color is sRGB-encoded; decode to linear so the sRGB
    // persistent target blends in linear. Coverage stays a plain alpha
    // multiplier (it is a coverage correction, not a color space one).
    return vec4<f32>(srgb_to_linear(in.color.rgb), in.color.a * cov);
}
