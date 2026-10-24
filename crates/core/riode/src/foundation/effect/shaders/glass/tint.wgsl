// Tier C6 — Tint (08.6): the flat tint mix.
//
// Taxonomy: L8 tint / absorption. A flat mix at the end of the shading
// chain — deliberately NOT path-length dependent, because the path
// length does not exist yet; `Absorb` (08.5 B5) is the layer that must
// replace this with Beer-Lambert once `stageRefract` publishes
// `path_length`. With `path_length == 0` Absorb degenerates to exactly
// this mix, so the migration stays pixel-identical with the knob off.
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
