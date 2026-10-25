// Tier D — Fresnel (A1): the Schlick edge glint.
//
// Taxonomy: L12 Fresnel reflection. First consumer of the D1 substrate
// widen: the bevel angle is `n3.z` — with the view on a UI plane taken
// straight-on, `dot(N, V) = n3.z`, and the Schlick weight
// `F = F0 + (1 − F0) · (1 − N·V)^power` reads the height field's slope
// where it is steep (the rim band) and nothing on the flat top. This
// is the `dot(N, V)` form `rim.wgsl`'s header books for the fake:
// Fresnel is its own light, separable from the rim's streaks — a
// stack item (`backdrop_effects=[Fresnel()]`, stage id 14), reorderable
// against `Rim()`/`Glow()`.
//
// Knobs ride the `L` bank's retired slots (`u.u_var[11]`): `.w` gain
// (`Glass::fresnel`), `.y` F0 base reflectance (`fresnel_f0`, default
// 0.2), `.z` the exponent (`fresnel_power`, default 5 = Schlick). Gain
// 0 = no-op, so absent stages and absent knobs both preserve the
// canonical look exactly.
//
// Reads: `n3`, `col`.
// Writes: `col`.

/// Schlick Fresnel: additive white glint scaled by the reflectance.
fn stageFresnel(g: GlassCtx) -> GlassCtx {
    var o = g;
    let L = u.u_var[11];
    if (L.w <= 0.0) {
        return o;
    }
    let f0 = select(0.2, L.y, L.y > 0.0);
    let p = select(5.0, L.z, L.z > 1.0);
    let nov = clamp(o.n3.z, 0.0, 1.0);
    let f = f0 + (1.0 - f0) * pow(1.0 - nov, p);
    o.col += vec3<f32>(L.w * f * o.coverage);
    return o;
}
