// Image sampling: premultiplied sRGB-encoded bytes (ImageSource convention,
// shared with tiny-skia/vello) -> linear premultiplied -> persistent target.
// Full-quad UVs let the hardware pick the mip level from screen-space
// derivatives (the CPU uploaded a full box-filtered mip chain), so
// minified images resolve smoothly instead of shimmering.
//
// The texture is plain Unorm, not srgb: hardware decode would apply the
// transfer curve to the *premultiplied* channels (decode(rgb*a) !=
// decode(rgb)*a), graying semi-transparent edges. Un-premultiply in
// encoded space, decode, re-premultiply — the standard correct path.
// Pipeline blends PREMULTIPLIED_ALPHA into the sRGB target (encodes on
// store). Vertex color is a straight-alpha tint: rgb modulates the
// premultiplied sample, alpha multiplies.
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

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    let s = textureSample(image, image_sampler, in.uv);
    // Premultiplied invariant: rgb <= a, so a == 0 implies rgb == 0 and
    // the max() below only guards the division.
    let a = s.a;
    let lin = srgb_to_linear(s.rgb / max(a, 1e-6)) * a;
    // Tint rgb modulates the premultiplied sample; tint alpha (opacity)
    // scales both color and alpha to keep the result premultiplied.
    return vec4<f32>(lin * in.color.rgb, a * in.color.a);
}

