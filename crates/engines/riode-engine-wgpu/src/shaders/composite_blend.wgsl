// Backdrop-blended layer composite: the finished (possibly filtered)
// layer offscreen composited against the BeginLayer parent snapshot at
// a CSS blend mode.
//
// Same geometry contract as `composite.wgsl` — layer and snapshot are
// surface-sized targets sharing device coords, so the quad maps 1:1
// onto texels (`texel = floor(in.pos.xy)`, textureLoad, no sampler) —
// but the fixed-function blend state is OFF: wgpu blend factors cannot
// express the backdrop-piecewise modes (`Overlay`, `SoftLight`, the
// non-separable four), so the shader samples the backdrop itself and
// emits the full CSS compositing result, backdrop contribution
// included. Concatenated after the blend libraries (pipeline.rs) —
// this file is the second consumer of `blendMode()`; `glass.wgsl` is
// the first.
//
// Source path is `composite.wgsl` verbatim up through the colour
// matrix (unpremultiply → matrix → clamp → re-premultiply by source
// alpha); then §"Compositing and Blending" of CSS Compositing Level 1:
//
//   co = αs(1−αb)Cs + αsαb·B(Cb,Cs) + (1−αs)αb·Cb   (premultiplied)
//   αo = αs + αb(1−αs)
//
// With `B = Cs` (mode `Normal`/0 — never used: Normal layers take the
// regular pipeline) the terms collapse to premultiplied source-over.
// Output is premultiplied; `in.color` is the [1,1,1,1] vertex tint slot
// kept for attribute-consumption parity.

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
    // .xy: sample-position shift (zero on this path — the content quad
    // reads unshifted texels); .z: blend mode wire id
    // (`BlendMode::as_uniform_id`, 0..15); .w: unused.
    tex_off: vec4<f32>,
};
@group(0) @binding(0)
var<uniform> ortho: Ortho;
@group(1) @binding(0)
var src_tex: texture_2d<f32>;
// The parent snapshot taken at BeginLayer: the CSS backdrop = whatever
// painted beneath this layer before it opened.
@group(1) @binding(1)
var dst_tex: texture_2d<f32>;
@group(1) @binding(2)
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
    // Backdrop at the fragment's own texel — UNSHIFTED. `c` carries the
    // shadow's `-offset` lookup shift (the silhouette lives undisplaced
    // in the layer texture), but the CSS backdrop is whatever paints
    // beneath this fragment — the same pixel the fixed-function path
    // would source-over onto. (On the layer-blend path `tex_off.xy` is
    // always zero, so both coordinates agree there.)
    let n = clamp(
        vec2<i32>(floor(in.pos.xy)),
        vec2<i32>(0),
        size - vec2<i32>(1),
    );
    let bd = textureLoad(dst_tex, n, 0);
    if (px.a <= 0.0) {
        // No source coverage: keep the attachment as-is. The parent
        // already holds the snapshot plus anything drawn since (same-
        // EndLayer shadow layers) — writing `bd` back would erase those.
        // For the layer path this is a no-op pixel either way (nothing
        // paints the parent between its snapshot and its composite
        // except its own shadows, which must survive).
        discard;
    }
    // Straight source through the layer colour matrix (identity for a
    // plain blend layer; a filtered+blended layer keeps the chain).
    let s = px.rgb / px.a;
    let m = vec4<f32>(s, px.a);
    var rgb = vec3<f32>(dot(cm.r0, m), dot(cm.r1, m), dot(cm.r2, m)) + cm.off.xyz;
    var a = dot(cm.r3, m) + cm.off.w;
    rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    a = clamp(a, 0.0, 1.0);

    // Backdrop straight; αb == 0 collapses the CSS terms to the source
    // alone (Cb is then multiplied by αb in every term, so its value
    // never leaks).
    let ab = bd.a;
    var cb = vec3<f32>(0.0);
    if (ab > 0.0) {
        cb = bd.rgb / ab;
    }
    let b = blendMode(cb, rgb, i32(cm.tex_off.z));
    let co = a * (1.0 - ab) * rgb + a * ab * b + (1.0 - a) * ab * cb;
    let ao = a + ab * (1.0 - a);
    return vec4<f32>(co * in.color.rgb, ao * in.color.a);
}
