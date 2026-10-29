// Tier B2 — Refract (08.5): the displacement half only.
//
// Taxonomy: L3 refraction (bending). L6 (slab thickness / volume) is
// published here: the Slab station folds into this tail — `path_length
// = v * band`, the bevel-band profile in device px (0 at the card
// centre, peaking where the lens bends light). `path_length` is the
// measured SDF field (never a constant), so `Absorb` (08.5 B5,
// Beer-Lambert) keys off true thickness — its knobs are the remaining
// work.
//
// Reads: `normal`, `depth`, `gradLen`, `halfMin`, `count`, `ownK`,
// `ownGrp`; `u_var[0]` (lens decay, band, strength), `u_var[2].w`
// (lens size ref), `u_var[8].yz` (lens edge px + edge decay px),
// `u_global2.z` (high-contrast), and the merge neck through
// `lensBlend` (shape.wgsl).
// Writes: `lensAmt`, `dLens`, `vv`, `sample_point`, `n3`, `path_length`.
//
// The two substrate outputs the widen adds (04 §4.1/4.2 made writable;
// the consumers — Fresnel/GGX, `Absorb`/`Slab` — are the next cuts):
//   * `n3` — the 3-D surface normal. Our lens field is implicitly a
//     height field whose horizontal gradient is the refraction: inverting
//     Snell at the reference's fitted ior gives the lift
//     ior/(ior-1) = 3 at ior 1.5 between height slope and displacement
//     slope, so the gradient magnitude is |dLens| x 3 along -normal
//     (the height rises inward). Normalized as
//     `normalize(vec3(-grad_xy, 1.0))`, matching `liquid-glass`'s
//     analytic normals (`glass.frag:301-302`).
//   * `path_length` — the optical travel through the glass. Slab (Step
//     2) folds into Refract's tail: `v * band`, the bevel-band profile
//     in device px (0 at the card centre, peaking where the glass bends
//     light, 0 past the band) — true thickness from the measured SDF
//     field, never a constant. Absent Refract publishes 0, which
//     is exactly the `Absorb`/`Slab` flat-mix degeneration (tint.wgsl)
//     and needs no knob.

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

    // Substrate: the 3-D normal and the optical path (see header).
    // The tilt is OUTWARD (the height falls toward the rim, so the
    // surface faces away from the centre — matching the reference's
    // `n = normalize(vec3(-grad, 1))`, where `grad` points inward).
    // `dLens` is positive (the inward displacement grows with depth),
    // so the outward tilt is `+normal * dLens * 3`, not negated: the
    // old `-` put the glint on the far bevel (NoL = 0 on the lit
    // side), which is why the ON card looked identical to OFF and the
    // light steer read inverted.
    o.n3 = normalize(vec3<f32>(o.normal * (o.dLens * 3.0), 1.0));
    // Slab (Step 2): the optical travel is the bevel-band profile, not
    // the inside depth — `v` is the normalized lens falloff (1 at the
    // rim, 0 past `band`), so `v * band` is a true thickness in device
    // px: 0 at the card centre, peaking in the bevel band where the
    // glass actually bends light, 0 past the band. That is the Slab
    // station folded into Refract's tail (no new dispatch id, no wire
    // change): Absorb keys its Beer–Lambert column off this, and at
    // centre (`v = 0`) the dye branch meets the flat mix exactly.
    o.path_length = v * band;
    return o;
}
