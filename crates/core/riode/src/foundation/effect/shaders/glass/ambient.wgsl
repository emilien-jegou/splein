// Tier C1 — Ambient (08.6): residual backdrop hue.
//
// Taxonomy: L9's companion — the glass keeps the backdrop's average
// chroma where the frosted sample went grey. It sits at the HEAD of
// Tier C's *reading* order only by tier label; in the chain it runs
// after `Tint` (the reference's order), so a tinted card keeps its full
// ambient chroma — see the `1 - sw` small-shape gate below.
//
// It is "optics-ish": it reads the backdrop average (25 taps over the
// blended rect), which is why it is the one Tier C stage with a
// `BackdropNeed`-style backdrop read of its own.
//
// Reads: `col`, `sw`, `rcB`; `u_var[30].xy` (ambient mix, ambient
// reach px).
// Writes: `col`.

/// Ambient: residual backdrop hue where the frosted sample went grey.
fn stageAmbient(g: GlassCtx) -> GlassCtx {
    var o = g;
    let P = u.u_var[30];
    // Ambient colour: the residual sat where our frosted backdrop is grey
    // but the glass carries the backdrop's average hue: + P.x x (avg -
    // luma(avg)), avg = 5 x 5 taps of the blurred texture over the
    // blended rect inflated by P.y px; off in the small-shape class (x (1
    // - sw)). After the tint mix: tinted glass keeps the full ambient
    // chroma. Skipped when 0.
    let am = P.x * (1.0 - o.sw);
    if (am != 0.0) {
        let a0 = o.rcB.xy - vec2<f32>(P.y);
        let asz = o.rcB.zw + vec2<f32>(2.0 * P.y);
        var acc = vec3<f32>(0.0);
        for (var i = 0; i < 5; i = i + 1) {
            for (var j = 0; j < 5; j = j + 1) {
                acc += tex(a0 + asz * (vec2<f32>(f32(i), f32(j)) + 0.5) * 0.2).rgb;
            }
        }
        acc *= 0.04;
        o.col += am * (acc - vec3<f32>(dot(acc, vec3<f32>(0.2126, 0.7152, 0.0722))));
    }
    return o;
}
