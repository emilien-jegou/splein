// Tier B2 — Refract (08.5): the displacement half only.
//
// Taxonomy: L3 refraction (bending). L6 (slab thickness / volume) is
// only *approximated* here: the lens `band` + `lens_decay` profile is
// what stands in for an optical path. There is no `path_length` in the
// ctx yet, so `Absorb` (08.5 B5, Beer-Lambert) cannot exist — that is
// the one output this stage will gain when it does.
//
// Reads: `normal`, `depth`, `gradLen`, `halfMin`, `count`, `ownK`,
// `ownGrp`; `u_var[0]` (lens decay, band, strength), `u_var[2].w`
// (lens size ref), `u_var[8].yz` (lens edge px + edge decay px),
// `u_global2.z` (high-contrast), and the merge neck through
// `lensBlend` (shape.wgsl).
// Writes: `lensAmt`, `dLens`, `vv`, `sample_point`.

/// Merge necks, exponential lens falloff, edge term: sets `sample_point`
/// (the displaced backdrop read), `lensAmt`, `dLens`, `vv`.
fn stageRefract(g: GlassCtx) -> GlassCtx {
    var o = g;
    let A = u.u_var[0];
    let I = u.u_var[8];
    let C = u.u_var[2];
    let hc = u.u_global2.z;

    // Merge necks: where the lens field's smooth union is active (smin
    // weight > 0) the lens treats the blob as one surface, after the
    // reference's blended geometry: depth is the field over its gradient
    // length (a true distance) and the displacement is scaled by that
    // length, so it fades to 0 at the neck's saddle instead of flipping
    // sides (the crease a unit normal draws). Outside merge necks neck = 1
    // and the lens is unchanged.
    var neck = 1.0;
    if (o.count > 1 && o.ownK > 0.0 && lensBlend(o.px, o.ownGrp) > 0.0) {
        neck = clamp(0.5 * o.gradLen, 0.05, 1.0);
    }

    // Lens v3: the displacement along the normal falls off exponentially
    // with depth (length `decay`) and is cut to 0 at `band`. At the edge
    // it is strength x band (about -47 pt: the outer pixels sample deep
    // inside, so the band shows a mirrored, compressed copy of the
    // interior). Shapes with a half shorter side below the size ref get a
    // uniformly scaled-down lens.
    let ldepth = o.depth / neck;
    let band = max(A.y, 1.0);
    let decay = max(A.x, 1e-3);
    let sc = select(1.0, min(1.0, o.halfMin / C.w), C.w > 0.0);
    let cut = exp(-band / decay);
    let ls = max(decay * sc, 1e-3);
    let ex = exp(-max(ldepth, 0.0) / ls);
    let v = max(ex - cut, 0.0) / max(1.0 - cut, 1e-6);
    let lensK = A.z * (1.0 - 0.5 * hc) * band * sc;
    o.lensAmt = lensK * v;
    // d(lensAmt)/d(depth), for the post-lens blur's Jacobian.
    o.dLens = select(0.0, -lensK * ex / ls / max(1.0 - cut, 1e-6), v > 0.0);
    // Lens edge term: an extra inward offset I.y x exp(-depth / I.z).
    let le = I.y * exp(-max(ldepth, 0.0) / max(I.z, 1e-3));
    o.lensAmt -= le;
    o.dLens += le / max(I.z, 1e-3);
    // d/ddepth of neck x f(depth / neck) is f'(depth / neck): dLens stays.
    o.lensAmt *= neck;
    o.vv = v;
    o.sample_point = o.px + o.normal * o.lensAmt;
    return o;
}
