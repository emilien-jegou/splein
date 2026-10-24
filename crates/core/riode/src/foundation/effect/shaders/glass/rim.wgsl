// Tier D/E — Rim (08.7): the specular streak + the isotropic edge light.
//
// Taxonomy: L10 specular lobe + L11 specular placement (approximated:
// `rim_back` gives a *back* rim, there is no inside/outside selector) +
// L13 rim / edge-bevel light + L12 Fresnel's absence (explicit: no
// `dot(N,V)`, no Schlick — `Fresnel`/`Specular` are 08.5 B6/B7 and do
// not exist yet; this is the dot-product fake they replace).
//
// Reads: `col`, `depth`, `normal`; `u_var[1].xy` (rim width, rim
// intensity), `u_var[9]` (rim mix, mix width, mix cut, luma floor),
// `u_var[11].x` (rim back strength), `u_global.z` (light angle),
// `u_global2.z` (high-contrast).
// Writes: `col`.

/// Rim lights: specular streak plus isotropic white mix.
fn stageRim(g: GlassCtx) -> GlassCtx {
    var o = g;
    let B = u.u_var[1];
    let J = u.u_var[9];
    let L4 = u.u_var[11];
    let hc = u.u_global2.z;
    let L = vec2<f32>(cos(u.u_global.z), sin(u.u_global.z));
    let rimW = max(B.x * (1.0 + hc), 0.5);
    let rim = 1.0 - smoothstep(0.0, rimW, o.depth);
    let spec = rim * (max(dot(o.normal, L), 0.0) + L4.x * max(dot(o.normal, -L), 0.0));
    o.col += B.y * (1.0 + hc) * spec;

    // Isotropic rim: a mix toward white, alpha J.x ramping to 0 at depth
    // J.y, cut at J.z; scaled by the glass luminance down to J.w (dark
    // mode).
    let ra = J.x * clamp(1.0 - o.depth / max(J.y, 1e-3), 0.0, 1.0)
        * (1.0 - smoothstep(J.z - 0.5, J.z + 0.5, o.depth));
    let rl = dot(o.col, vec3<f32>(0.2126, 0.7152, 0.0722));
    let ra0 = ra * (J.w + (1.0 - J.w) * clamp(rl / 0.5, 0.0, 1.0));
    o.col += (vec3<f32>(1.0) - o.col) * ra0;
    return o;
}
