// Tier D — GGX (A2): the directional microfacet glint.
//
// Taxonomy: L10 specular lobe done properly — this is what replaces
// `rim.wgsl`'s `dot(normal, L)` streak fake (Rim stays as the
// non-photometric accent; canonical order runs GGX after Fresnel).
// Port of `liquid-glass`'s GGX glint (`glass.frag:398-409`) with its
// point light collapsed to an authored direction: the reference lights
// from `uLight` (`Ld = normalize(uLight - P3)`, a cursor lamp); ours
// takes the direction directly. The reference's HDR studio intensity
// (`uLightInt`, "a light strip has to be ~25x brighter than the scene")
// is folded into the authored `gain`.
//
// Reads: `col`, `n3` (the D1 normal — Refract's Snell lift, identity
// (0,0,1) without it), `vv` (Refract's lens profile: 1 at the outline,
// decaying inward, 0 without Refract), `coverage`.
// Writes: `col`.
// Knobs ride the appended GGX tail (`u_ggx`, wgpu-only — after the
// stage list, past the frozen v5 wire): [0].x roughness α (0 = shader
// default 0.2), [0].y gain (0 = no-op), [0].zw + [1].x the light
// direction xyz, surface→light in surface coords (x right, y down, z
// out of the screen; zero length = the default key light, upper-left
// front — the reference's `-3π/4` key light direction).
fn stageGGX(g: GlassCtx) -> GlassCtx {
    var o = g;
    let K = u.u_ggx[0];
    if (K.y <= 0.0) {
        return o;
    }
    let ld0 = vec3<f32>(K.z, K.w, u.u_ggx[1].x);
    var ld = vec3<f32>(-0.577, -0.577, 0.577);
    if (dot(ld0, ld0) > 1e-6) {
        ld = normalize(ld0);
    }
    let rough = select(0.2, K.x, K.x > 0.0);
    let V = vec3<f32>(0.0, 0.0, 1.0);
    let H = normalize(ld + V);
    let NoH = max(dot(o.n3, H), 0.0);
    let NoL = max(dot(o.n3, ld), 0.0);
    let a = rough * rough;
    let a2 = a * a;
    let dn = NoH * NoH * (a2 - 1.0) + 1.0;
    let D = a2 / (3.14159265 * dn * dn);
    let VoH = clamp(dot(V, H), 0.0, 1.0);
    let Fh = 0.04 + 0.96 * pow(1.0 - VoH, 5.0);
    let NoV = clamp(o.n3.z, 0.0, 1.0);
    var spec = D * Fh * NoL / (4.0 * max(NoV, 0.25));
    // The glint lives on the bevel band (the reference gates on its
    // `bev` field, `smoothstep(0.08, 0.6, bev)`). Ours is `vv` —
    // Refract's lens profile, 1 at the outline and 0 without a Refract
    // (no bevel, no glint: on the flat face NoL/NoH are constants and
    // the "glint" would be a uniform brightness lift, which is not a
    // glint).
    spec *= smoothstep(0.08, 0.6, o.vv);
    o.col += vec3<f32>(K.y * spec * o.coverage);
    return o;
}
