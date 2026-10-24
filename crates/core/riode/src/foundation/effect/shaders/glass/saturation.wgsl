// Tier C2 — Saturation (08.6): the luma-keyed chroma grade.
//
// Taxonomy: L9 saturation / vibrance. Split out of the former
// `stageTone`, which fused seven layers into one function: this is the
// first of them, and it is now dialable (and reorderable against the
// tone curve) on its own — `backdrop_effects=[glass(), Saturate::new(1.3)]`
// changes only this line and the tone curve survives.
//
// Order is the fused fn's, kept: Saturation runs before the tone curve,
// the wash, the dim, the small-shape curve and the tint.
//
// Reads: `col`; `u_var[3].w` (saturation, 1.0 = untouched).
// Writes: `col`.

fn stageSaturation(g: GlassCtx) -> GlassCtx {
    var o = g;
    let D = u.u_var[3];
    let luma = dot(o.col, vec3<f32>(0.2126, 0.7152, 0.0722));
    o.col = mix(vec3<f32>(luma), o.col, D.w);
    return o;
}
