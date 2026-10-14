// Glyph coverage sampling: atlas R channel through the display
// transfer function, then * vertex alpha.
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

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    var cov = textureSample(atlas, atlas_sampler, in.uv).r;
    if (in.gam > 0.5) {
        cov = gamma_lut(cov);
    }
    return vec4<f32>(in.color.rgb, in.color.a * cov);
}
