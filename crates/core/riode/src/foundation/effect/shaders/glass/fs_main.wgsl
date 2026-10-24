// Tier P — Orchestrator. `fs_main` is purely an orchestrator:
// silhouette, then one function per material stage in pipeline order.
// Every stage takes a `GlassCtx` and returns it; the math inside each
// stage is verbatim what the monolithic `fs_main` did (same taps, same
// order), so reordering stages is a one-line change here and a custom
// effect can replace a single stage without forking the file.
//
// Uniform banks are read directly from `u` inside each stage
// (`u.u_var[N]`); the context carries only per-pixel varyings.
//
// Canonical order (08.6): the Shape/Refract pair first (the normal must
// exist before anything refracts), then the Tier B optics, then the
// Tier C colour chain in the order the reference ran it inside its
// former fused `stageTone` — `Saturation, ToneCurve, Wash, Dim, Tint` —
// with the small-shape tone curve between `Dim` and `Tint` (08.6's
// `stageShapeTone`), then `Ambient` (after the tint, so tinted glass
// keeps the full ambient chroma), then Tier D lighting, then
// `cardBlend`. Tier E composites outside the loop: the card blend runs
// on the finished colour.
//
// Explicit fold: a stack that authored unit items packs its stage list
// into `u_stages` (count > 0) and this orchestrator runs exactly those
// stages in author order instead of the canonical chain. Shape always
// runs first and cardBlend always last — they are not listable stages.
// Count 0 (a `glass()`-only stack, or the wire) takes the canonical
// branch below, byte-identical to the pre-fold shader.

// Stage id at 0-based index `i` of the explicit list packed in
// `u_stages` (count in [0].x, then ids across the four vec4s).
fn stageIdAt(i: i32) -> i32 {
    let k = i + 1;
    if (k <= 3) {
        if (k == 1) { return i32(u.u_stages[0].y); }
        if (k == 2) { return i32(u.u_stages[0].z); }
        return i32(u.u_stages[0].w);
    } else if (k <= 7) {
        if (k == 4) { return i32(u.u_stages[1].x); }
        if (k == 5) { return i32(u.u_stages[1].y); }
        if (k == 6) { return i32(u.u_stages[1].z); }
        return i32(u.u_stages[1].w);
    } else if (k <= 11) {
        if (k == 8) { return i32(u.u_stages[2].x); }
        if (k == 9) { return i32(u.u_stages[2].y); }
        if (k == 10) { return i32(u.u_stages[2].z); }
        return i32(u.u_stages[2].w);
    } else {
        if (k == 12) { return i32(u.u_stages[3].x); }
        return i32(u.u_stages[3].y);
    }
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // Output is premultiplied colour composited srcOver onto the sharp
    // backdrop: transparent outside the shape.
    let px = pos.xy;
    let count = i32(u.u_global.x);
    if (count == 0) {
        return vec4<f32>(0.0);
    }

    // Outside the silhouette the optics pass emits nothing: the halo is
    // the renderer's theatre-mode replay (blurred finished card under the
    // sharp one, `refraction_replay`), not shading. A shader-side halo
    // could only re-evaluate the lobe field — a second gradient that
    // diverges from the card by construction — so it was deleted.
    var g = glassSilhouette(px, count);
    if (g.keep <= 0.0) {
        return vec4<f32>(0.0);
    }
    if (g.coverage <= 0.0) {
        return vec4<f32>(0.0);
    }

    // Opaque (Reduce Transparency).
    if (u.u_global.w > 0.5) {
        let solid = mix(u.u_opaque.rgb, g.tint.rgb, g.tint.a);
        return cardBlend(solid * (g.coverage * g.keep), g.coverage * g.keep, px);
    }

    g = stageShape(g);
    let stage_count = i32(u.u_stages[0].x);
    if (stage_count <= 0) {
        // Canonical chain (a `glass()`-only stack): every stage, in the
        // reference's fixed order.
        g = stageRefract(g);
        g = stageFrost(g);
        g = stageSpectral(g);
        g = stageSaturation(g);
        g = stageToneCurve(g);
        g = stageWash(g);
        g = stageDim(g);
        g = stageShapeTone(g);
        g = stageTint(g);
        g = stageAmbient(g);
        g = stageGlow(g);
        g = stageRim(g);
        g = stagePress(g);
    } else {
        // Explicit fold: the author's stage list, in author order.
        for (var i = 0; i < stage_count; i = i + 1) {
            switch stageIdAt(i) {
                case 1 { g = stageRefract(g); }
                case 2 { g = stageFrost(g); }
                case 3 { g = stageSpectral(g); }
                case 4 { g = stageSaturation(g); }
                case 5 { g = stageToneCurve(g); }
                case 6 { g = stageWash(g); }
                case 7 { g = stageDim(g); }
                case 8 { g = stageShapeTone(g); }
                case 9 { g = stageTint(g); }
                case 10 { g = stageAmbient(g); }
                case 11 { g = stageGlow(g); }
                case 12 { g = stageRim(g); }
                case 13 { g = stagePress(g); }
                default {}
            }
        }
    }

    return cardBlend(
        clamp(g.col, vec3<f32>(0.0), vec3<f32>(1.0)) * (g.coverage * g.keep),
        g.coverage * g.keep,
        px,
    );
}
