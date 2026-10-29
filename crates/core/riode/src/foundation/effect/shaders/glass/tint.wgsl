// Tier C6 — Tint / Absorb (08.6 + Step 2): the flat tint mix, grown
// Beer–Lambert dye on top.
//
// Taxonomy: L8 tint / absorption. The flat `mix` stays the canonical
// knob-off look; `density > 0` (per-shape `u_info.y`, packed from
// `Backdrop::absorb_density`) turns the mix into a Beer–Lambert dye
// keyed off the true bevel-band path length the Slab station publishes
// in `stageRefract`'s tail (`path_length = v * band` in device px — 0
// at the card centre, peaking in the bevel, 0 without Refract, which
// is exactly the flat-mix degeneration).
//
// Degeneration contract (all three must hold; the first two are the
// knob-off pixel-identical promise):
//   * `density == 0` → exactly `mix(col, tint.rgb, tint.a)`.
//   * `path_length == 0` → the dye column vanishes (`exp(0) = 1`), so
//     the branch IS the flat mix — no seam where the lens ridge starts.
//   * `tint.a == 0` → `col` untouched on both branches.
//
// The dye branch keeps the same outer blend weight as the flat mix:
// `col = mix(col, transmitted, tint.a)` where `transmitted` is the
// backdrop colour after the dye column `tint.rgb` absorbs over the path.
// At zero path `exp(0) = 1` and the branch IS the flat mix — no seam
// where the lens ridge starts.
//
// Reads: `col`, `tint` + `density` (per-shape, proximity-blended in
// `glassSilhouette`), `path_length` (from `stageRefract`).
// Writes: `col`.

fn stageTint(g: GlassCtx) -> GlassCtx {
    var o = g;
    if (o.density <= 0.0) {
        o.col = mix(o.col, o.tint.rgb, o.tint.a);
        return o;
    }
    // Beer–Lambert: the dye column absorbs per channel over the path.
    // `transmit_beer` is the material-DSL twin (`material.rs` PRELUDE):
    // density <= 0 returns `c * tint`; here the outer `mix` by `tint.a`
    // plays that role, so the transmitted colour is `c * exp(-absorb)`
    // with `absorb = -log(clamp(tint.rgb)) * density * path`.
    let absorb = -log(clamp(o.tint.rgb, vec3<f32>(1e-3), vec3<f32>(1.0))) * o.density * o.path_length;
    let transmitted = o.col * exp(-absorb);
    o.col = mix(o.col, transmitted, o.tint.a);
    return o;
}
