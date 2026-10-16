// Glyph coverage sampling: plain alpha coverage from the R8 atlas,
// straight through, then * vertex alpha.
//
// Twin of glyph.wgsl with the display-correction chain bypassed
// (vello-mode experiment): coverage reaches the blend unmodified, like
// vello's straight rgba×coverage composite — smooth, slightly soft,
// nothing amplified. Selected at build time by pipeline.rs when
// RIODE_TEXT_CORRECTION=off. Keep the vertex I/O, bindings, and blend
// contract identical to glyph.wgsl; keep this file in sync with it.
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
    let cov = textureSample(atlas, atlas_sampler, in.uv).r;
    // Encoded-space composite on the plain Unorm target (vello-like):
    // vertex color rides through, alpha carries coverage.
    return vec4<f32>(in.color.rgb, in.color.a * cov);
}
