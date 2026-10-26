// Tier D — EnvReflect (A10): the backdrop mirror + sky at steep angles.
//
// Taxonomy: A10 environment reflection. The one piece of the reference
// reflection block (`liquid-glass` `glass.frag:370-396`) nobody owns:
// `R = reflect(I, n)`, the steep-rim mirror fetch of the backdrop just
// outside the glass, the sky gradient on the flat face, and — the part
// that separates it from every additive fake — the Fresnel-weighted
// blend `col·(1-F) + env·F`, which DARKENS the transmitted image on the
// bevel where the mirror takes over.
//
// What stays out (deliberately, for a later cut):
//   * the studio strips (`uRim·24` / `uRim2·6` × `band`, `glass.frag:392`)
//     — env-space terms with HDR intensity semantics (`uLightInt`);
//     they ride as knobs on this stage later, not as Rim extensions.
//     `rim.wgsl` keeps only its legacy 2-D streak fake.
//   * the cursor-lamp mirror (`uEnv·14·pow(dot(R,Ld),4000)`) — needs a
//     light position the engine does not ship yet.
//   * the overhead softbox sheen (`uSky·18·smoothstep(0.75,1,R.z)`) —
//     folded into the sky gradient's flat-face lift for now.
//
// Reads: `col`, `n3`, `px` (mirror fetch position); `u_env` tail:
// `[0].x` gain (0 = no-op), `[0].y` sky strength, `[0].z` the mirror
// reach cap in px (travel clamp, like the reference's 160), `[0].w` the
// sky horizon lift, `[1].x` the sky zenith gain.
// Writes: `col` (the F-blended mirror composite).

/// Environment reflection: mirror of the backdrop on the steep rim,
/// sky lift on the flat face, Fresnel-weighted blend.
fn stageEnvReflect(g: GlassCtx) -> GlassCtx {
    var o = g;
    let K = u.u_env[0];
    if (K.x <= 0.0) {
        return o;
    }
    let I = vec3<f32>(0.0, 0.0, -1.0);
    let R = reflect(I, o.n3);
    let NoV = clamp(o.n3.z, 0.0, 1.0);
    let F = 0.04 + 0.96 * pow(1.0 - NoV, 5.0);
    var env: vec3<f32>;
    if (R.z < 0.0) {
        // Steep rim: mirror of the backdrop just outside the glass. The
        // fetch walks R.xy from this pixel, clamped to the same travel
        // cap the reference uses — the rim reads content from across
        // the outline, mirrored and compressed.
        let reach = select(160.0, K.z, K.z > 0.0);
        let t = reach / max(-R.z, 0.05);
        env = tex(o.px + R.xy * min(t, reach)).rgb * 0.9;
    } else {
        // Gentle slope / flat face: sky gradient, brighter overhead.
        // Display-referred: `sky` is the absolute face colour (the
        // reference's uSky rides an HDR exposure stage we do not have,
        // so it needs no 0.01-scale constant — author it directly).
        let sky = select(0.6, K.y, K.y > 0.0);
        let horizon = select(0.6, K.w, K.w > 0.0);
        let zenith = select(1.2, u.u_env[1].x, u.u_env[1].x > 0.0);
        env = vec3<f32>(sky * (horizon + zenith * R.z));
    }
    o.col = mix(o.col, o.col * (1.0 - F) + env * (F * K.x), o.coverage);
    return o;
}
