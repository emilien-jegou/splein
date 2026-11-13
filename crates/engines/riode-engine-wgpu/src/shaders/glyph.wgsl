// Glyph coverage sampling: pure linear-light source-over.
//
// Stores the exact geometric area coverage from the analytic rasterizer
// (R8 atlas, unmodified coverage — no contrast curves, no gamma tables).
// The vertex color arrives already converted to linear RGB at the CPU
// boundary (command.rs); the fragment computes straight alpha
// (rgb unchanged, alpha = vertex alpha * coverage) and the sRGB-tagged
// render target performs the exact linear-to-sRGB transfer on store.
// Blending factors (SrcAlpha / OneMinusSrcAlpha) match the straight-alpha
// output; every blend step executes in linear light.
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

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    let coverage = textureSample(atlas, atlas_sampler, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
