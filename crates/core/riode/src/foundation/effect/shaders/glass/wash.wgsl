// Tier C4 — Wash (08.6): the fill wash over the glass body.
//
// Taxonomy: L7 translucency / density — the medium's own colour and how
// much of it sits in front of the backdrop (a flat overlay, not a
// Beer-Lambert path term; the path term is `Absorb`, 08.5 B5, which
// does not exist yet).
//
// Order is the fused fn's, kept: after `ToneCurve`, before `Dim`.
//
// Reads: `col`, `fillScale`; `u_var[1].z` (fill opacity),
// `u_var[3].xyz` (fill rgb).
// Writes: `col`.

fn stageWash(g: GlassCtx) -> GlassCtx {
    var o = g;
    let B = u.u_var[1];
    let D = u.u_var[3];
    o.col = mix(o.col, D.rgb, B.z * o.fillScale);
    return o;
}
