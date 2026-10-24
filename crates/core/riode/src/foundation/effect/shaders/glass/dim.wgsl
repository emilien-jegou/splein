// Tier C5 — Dim (08.6): the post-tone multiply.
//
// Taxonomy: L7's second half — how much light the slab keeps back.
//
// Order is the fused fn's, kept: after `Wash`, before `stageShapeTone`.
//
// Reads: `col`; `u_var[1].w` (dim, 0..1).
// Writes: `col`.

fn stageDim(g: GlassCtx) -> GlassCtx {
    var o = g;
    let B = u.u_var[1];
    o.col *= (1.0 - B.w);
    return o;
}
