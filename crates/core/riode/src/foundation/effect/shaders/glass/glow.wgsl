// Tier D — Glow (08.7): the emissive voice-glow wash inside the slab.
//
// Taxonomy: L17 bloom / glow — an emissive wash, not a bloom pass (no
// pyramid, no threshold): chromaticity and intensity are separated so
// the display-referred pipeline does not clip it to invisible white.
//
// The gradient lives in userland (`Glow`: stops, anchor, span,
// thickness); the CPU resolves it to the uniform lobes at pack time and
// this module only evaluates them — no palette or geometry is
// hardcoded. Interior only: the halo outside the edge is `Refraction`,
// a separate renderer-owned layer (`LightSpill`), not a stage here.
//
// Reads: `px`, `sample_point`, `col`; `u_var[32].w` (voice-glow gain —
// 0 takes the no-glow path), `u_var[34].x` (glow inside gain),
// `u_glow_lobes`/`u_glow_cols`/`u_glow_geom` (lobe field: line, sigma,
// refracted flag, blend id).
// Writes: `col`.

// Voice glow: the emissive wash along the anchor edge (faithful
// `glowField`: voice lobes, Gaussian in both axes, sampled at the
// geometric pixel unless the refracted flag asks for the lens-displaced
// point. The gradient lives
// in userland (`Glow`: stops, anchor, span, thickness); the CPU resolves
// it to these uniform lobes at pack time and the shader only evaluates
// them — no palette or geometry is hardcoded. Interior only: the halo
// outside the edge is `Refraction`, a separate layer-owned concept.
//
// Display-referred adaptation: the reference adds (`bgFetch + glow·inside`)
// and survives because its backdrops sit at midtones and a filmic tonemap
// compresses highlights downstream — neither holds here (white field,
// hard clamp), so addition clips to invisible white. Instead chromaticity
// and intensity are separated: the returned rgb is the normalized weighted
// lobe mean (≤1, unshaped — saturation as a glow knob is deleted) and the
// alpha is the envelope × gain.
//
// Geometry mirrors the reference: the lobe line sits just outside the
// edge (`u_glow_geom.x`), cross-sigma ~0.17× the cross extent, lobe
// sigma ~1.6× the lobe spacing, amplitudes carry the `exp(-nj²/0.32)`
// edge falloff (see `Glow::resolve`).
fn glowOverlay(p: vec2<f32>, gain: f32) -> vec4<f32> {
    let along = p.x;
    let cross = (p.y - u.u_glow_geom.x) / max(u.u_glow_geom.y, 1.0);
    let cross2 = cross * cross;
    var num = vec3<f32>(0.0);
    var den = 0.0;
    for (var j = 0; j < 6; j = j + 1) {
        let L = u.u_glow_lobes[j];
        let d = (along - L.x) / max(L.y, 1.0);
        let wgt = L.z * exp(-d * d - cross2);
        num += u.u_glow_cols[j].rgb * wgt;
        den += wgt;
    }
    let rgb = num / max(den, 1e-4);
    return vec4<f32>(rgb, clamp(den * gain, 0.0, 1.0));
}

// The interior wash: the lens-independent emissive evaluation — geometric
// pixel, no bent or mirrored sampling. Alpha-weighted vertical taps blend
// neighbouring lobes into one continuous wash instead of separated plumes.
fn glowWash(p: vec2<f32>, gain: f32) -> vec4<f32> {
    let off = max(u.u_glow_geom.y, 1.0) * 0.75;
    let s0 = glowOverlay(p, gain);
    let s1 = glowOverlay(p + vec2<f32>(0.0, off), gain);
    let s2 = glowOverlay(p - vec2<f32>(0.0, off), gain);
    let asum = max(s0.a + s1.a + s2.a, 1e-4);
    let rgb = (s0.rgb * s0.a + s1.rgb * s1.a + s2.rgb * s2.a) / asum;
    return vec4<f32>(rgb, asum / 3.0);
}

/// Voice glow inside the slab, composited over the glass base.
fn stageGlow(g: GlassCtx) -> GlassCtx {
    var o = g;
    let Q = u.u_var[32];
    let R4 = u.u_var[34];
    // Voice glow inside the slab: the shared wash (`glowWash`) composited
    // over the glass base. Reference behaviour samples it at the
    // geometric pixel, unaffected by the lens; the opt-in deviation
    // (`Glow::refracted`, carried in `u_glow_geom.z`) samples it at the
    // lens-displaced point, so the wash bends with the refraction like
    // light inside the glass. The wash composites at `Glow::blend`
    // (`u_glow_geom.w`): Normal (0) is the plain source-over `mix`
    // (blendMode returns src → mix(col, src, ov.a)), the other modes
    // tone the base like ink on glass. Before the rim lights, which stay
    // white.
    let glowGain = Q.w;
    let glowIn = R4.x;
    if (glowGain > 0.0) {
        let gp = select(o.px, o.sample_point, u.u_glow_geom.z > 0.5);
        let ov = glowWash(gp, glowGain * glowIn);
        o.col = mix(o.col, blendMode(o.col, ov.rgb, i32(u.u_glow_geom.w)), ov.a);
    }
    return o;
}
