// Tier C6 — Tint (08.6): the flat tint mix.
//
// Taxonomy: L8 tint / absorption. A flat mix at the end of the shading
// chain — deliberately NOT path-length dependent: the flat mix is the
// canonical look; `Absorb` (08.5 B5) is the layer that must replace it
// with Beer-Lambert once `path_length` is consumed (published since
// the D1 widen, `refract.wgsl`). With `path_length == 0` Absorb
// degenerates to exactly this mix, so the migration stays
// pixel-identical with the knob off.
//
// Order is the fused fn's, kept: last of the six, after
// `stageShapeTone` and before `stageAmbient`.
//
// Reads: `col`, `tint` (per-shape, proximity-blended in
// `glassSilhouette`).
// Writes: `col`.

fn stageTint(g: GlassCtx) -> GlassCtx {
    var o = g;
    o.col = mix(o.col, o.tint.rgb, o.tint.a);
    return o;
}
