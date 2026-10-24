// Tier C3 — ToneCurve (08.6): the tone LUT, its luma-keyed variant,
// the small-shape shadow lift, and the small-shape tone curve.
//
// Taxonomy: L8 tint/absorption's *curve* half and the display-grade
// half of the former `stageTone`. The small-shape curve runs AFTER the
// wash and the dim (that is where the reference ran it, and moving it
// would change pixels), so this module contributes two stages to the
// chain — `stageToneCurve` before `wash.wgsl`/`dim.wgsl`, and
// `stageShapeTone` after them. `stageShapeTone` also publishes `sw`, the
// small-shape weight `stageAmbient` keys its pickup on.
//
// Reads (both): `col`, `halfMin`; `u_var[6]`/`u_var[7]`/`u_var[8].x`
// (9 tone knots), `u_var[8].w` + `u_var[10].xy` (shadow lift + knee +
// size ref), `u_var[30].z` (luma-keyed tone mix), `u_var[24]`/
// `u_var[25]`/`u_var[26].x` (small-shape knots), `u_var[26].yz`
// (smallSizeLo/Hi px).
// Writes: `col`, `sw`.

// Tone LUT: 9 grey output knots at inputs i/8, hat-sum form (no dynamic
// array indexing, so it is safe everywhere: the knots travel as two
// vec4s and a float). Identity knots reproduce c. Used by the main tone
// stage (G, H, I.x) and the small-shape tone curve (M, N, O.x).
fn toneLut(c: vec3<f32>, a: vec4<f32>, b: vec4<f32>, k8: f32) -> vec3<f32> {
    let c8 = c * 8.0;
    return vec3<f32>(a.x)
        + (vec3<f32>(a.y) - vec3<f32>(a.x)) * clamp(c8, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(a.z) - vec3<f32>(a.y)) * clamp(c8 - 1.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(a.w) - vec3<f32>(a.z)) * clamp(c8 - 2.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.x) - vec3<f32>(a.w)) * clamp(c8 - 3.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.y) - vec3<f32>(b.x)) * clamp(c8 - 4.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.z) - vec3<f32>(b.y)) * clamp(c8 - 5.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(b.w) - vec3<f32>(b.z)) * clamp(c8 - 6.0, vec3<f32>(0.0), vec3<f32>(1.0))
        + (vec3<f32>(k8) - vec3<f32>(b.w)) * clamp(c8 - 7.0, vec3<f32>(0.0), vec3<f32>(1.0));
}

/// Tone LUT + luma-keyed variant + small-shape shadow lift.
fn stageToneCurve(g: GlassCtx) -> GlassCtx {
    var o = g;
    let G = u.u_var[6];
    let H = u.u_var[7];
    let I = u.u_var[8];
    let K = u.u_var[10];
    let P = u.u_var[30];

    // Tone LUT: 9 grey knots at inputs i/8, hat-sum form, on the frosted
    // backdrop before the fill wash; identity knots reproduce col.
    // Luma-keyed tone: P.z of the way toward moving luma through the LUT
    // and carrying the chroma along unchanged.
    var toned = toneLut(o.col, G, H, I.x);
    if (P.z != 0.0) {
        let y = dot(o.col, vec3<f32>(0.2126, 0.7152, 0.0722));
        let keyed = o.col + vec3<f32>(toneLut(vec3<f32>(y), G, H, I.x).x - y);
        toned = mix(toned, keyed, P.z);
    }
    o.col = toned;

    // Small-shape shadow lift: the dark end lifts behind small dark glass.
    // + I.w x size x max(0, 1 - col / K.x)^2, size = max(0, 1 - halfMin /
    // K.y).
    let liftSz = select(
        0.0,
        max(0.0, 1.0 - o.halfMin / max(K.y, 1e-3)),
        K.y > 0.0,
    );
    let lk = max(vec3<f32>(1.0) - o.col / max(K.x, 1e-3), vec3<f32>(0.0));
    o.col += (I.w * liftSz) * lk * lk;
    return o;
}

/// Small-shape tone curve: small shapes (half shorter side <= ~32 pt)
/// switch to a steeper tone response. Applied after the fill wash and
/// dim, weighted from 1 at O.y (smallSizeLo) to 0 at O.z (smallSizeHi);
/// off when smallSizeHi <= 0. Publishes `sw` for `stageAmbient`.
fn stageShapeTone(g: GlassCtx) -> GlassCtx {
    var o = g;
    let M = u.u_var[24];
    let N = u.u_var[25];
    let O = u.u_var[26];
    o.sw = 0.0;
    if (O.z > 0.0) {
        o.sw = clamp((O.z - o.halfMin) / max(O.z - O.y, 1e-3), 0.0, 1.0);
        o.col = mix(o.col, toneLut(o.col, M, N, O.x), o.sw);
    }
    return o;
}
