// Composites a (possibly blurred) element-filter layer offscreen onto
// its parent target.
//
// Layer and parent are surface-sized targets sharing device coords, so
// the quad maps 1:1 onto offscreen texels: texel = floor of the
// fragment's framebuffer position (the blit's textureLoad trick, no
// sampler).
//
// The offscreen holds premultiplied values (scene content alpha-blended
// over a transparent clear accumulates premultiplied), so the matrix —
// like CSS filter matrices — runs on unpremultiplied RGB, then clamps
// and re-premultiplies for PREMULTIPLIED_ALPHA_BLENDING. `a == 0` early
// returns (transparency stays transparency; every CSS color matrix keeps
// alpha identity, and unpremultiplying 0 would be NaN). Vertex color is
// a [1,1,1,1] tint slot kept for attribute-consumption parity.

struct Ortho {
    w: f32,
    h: f32,
};
struct ColorMatrix {
    r0: vec4<f32>,
    r1: vec4<f32>,
    r2: vec4<f32>,
    r3: vec4<f32>,
    off: vec4<f32>,
    // Sample-position shift: the alpha-shadow composite draws its quad at
    // the shadow's device offset while reading the source's un-shifted
    // texels (zero for normal content composites).
    tex_off: vec4<f32>,
};
@group(0) @binding(0)
var<uniform> ortho: Ortho;
@group(1) @binding(0)
var src_tex: texture_2d<f32>;
@group(1) @binding(1)
var<uniform> cm: ColorMatrix;

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
        vec2<i32>(floor(in.pos.xy + cm.tex_off.xy)),
        vec2<i32>(0),
        size - vec2<i32>(1),
    );
    let px = textureLoad(src_tex, c, 0);
    if (px.a <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let s = px.rgb / px.a;
    // Fourth component is the source alpha (not 1.0): the alpha row of
    // the matrix scales with it (identity keeps px.a; the shadow tint
    // row [0,0,0,op] yields px.a * op). A constant 1.0 here flattens
    // every alpha row to a constant — an alpha shadow then paints its
    // whole quad uniformly instead of fading with the blurred alpha.
    let m = vec4<f32>(s, px.a);
    var rgb = vec3<f32>(dot(cm.r0, m), dot(cm.r1, m), dot(cm.r2, m)) + cm.off.xyz;
    var a = dot(cm.r3, m) + cm.off.w;
    rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    a = clamp(a, 0.0, 1.0);
    return vec4<f32>(rgb * a * in.color.rgb, a * in.color.a);
}
