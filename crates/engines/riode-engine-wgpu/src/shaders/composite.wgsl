// Composites a layer offscreen onto its parent target.
//
// Layer and parent are surface-sized targets sharing device coords, so
// the quad maps 1:1 onto offscreen texels: texel = floor of the
// fragment's framebuffer position (the blit's textureLoad trick, no
// sampler).
//
// Both targets are sRGB-tagged holding premultiplied LINEAR values (scene
// content alpha-blended over a transparent clear accumulates
// premultiplied). textureLoad on an sRGB format returns decoded linear
// RGB (Vulkan/WebGPU decode on every sampled read, fetch included), so
// the composite runs in linear light; the sRGB parent target re-encodes
// on store. Vertex color is a [1,1,1,1] tint slot kept for
// attribute-consumption parity.

struct Ortho {
    w: f32,
    h: f32,
};
@group(0) @binding(0)
var<uniform> ortho: Ortho;
@group(1) @binding(0)
var src_tex: texture_2d<f32>;

struct Vin {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: Vin) -> Vout {
    var out: Vout;
    let nx = (in.pos.x / ortho.w) * 2.0 - 1.0;
    let ny = 1.0 - (in.pos.y / ortho.h) * 2.0;
    out.pos = vec4<f32>(nx, ny, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(src_tex, 0));
    let c = clamp(
        vec2<i32>(floor(in.pos.xy)),
        vec2<i32>(0),
        size - vec2<i32>(1),
    );
    let px = textureLoad(src_tex, c, 0);
    return vec4<f32>(px.rgb * in.color.rgb, px.a * in.color.a);
}
