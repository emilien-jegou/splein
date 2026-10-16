// Image sampling: premultiplied sRGB-encoded bytes (ImageSource convention,
// shared with tiny-skia/vello) composited as-is on the plain Unorm target
// (vello-like, no transfer handling). Full-quad UVs let the hardware pick
// the mip level from screen-space derivatives (the CPU uploaded a full
// box-filtered mip chain), so minified images resolve smoothly instead of
// shimmering. Pipeline blends PREMULTIPLIED_ALPHA; vertex color is a
// straight-alpha tint: rgb modulates the premultiplied sample, alpha
// multiplies.
struct Ortho {
    w: f32,
    h: f32,
};
@group(0) @binding(0)
var<uniform> ortho: Ortho;
@group(1) @binding(0)
var image: texture_2d<f32>;
@group(1) @binding(1)
var image_sampler: sampler;

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
    let s = textureSample(image, image_sampler, in.uv);
    // Tint rgb modulates the premultiplied sample; tint alpha (opacity)
    // scales both color and alpha to keep the result premultiplied.
    return vec4<f32>(s.rgb * in.color.rgb, s.a * in.color.a);
}

