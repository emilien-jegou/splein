// Tier D — Press (08.7): pressed-state lighting.
//
// Taxonomy: the interaction-response cross-cutting layer — the whole-
// shape press lift plus the Gaussian under the finger. Both follow the
// press light, not the press spring.
//
// Reads: `col`, `light`, `px`; `u_var[34].z` (press lift),
// `u_var[5].y` (touch glow strength), `u_touch` (x, y, glow, radius px).
// Writes: `col`.

/// Pressed-state lighting: whole-shape lift plus touch Gaussian.
fn stagePress(g: GlassCtx) -> GlassCtx {
    var o = g;
    let F = u.u_var[5];
    let R4 = u.u_var[34];
    // Pressed glass lights the whole shape (R.z) and, more, the area
    // under the finger (the Gaussian below); both follow the press light,
    // not the press spring.
    o.col += R4.z * o.light;
    if (u.u_touch.z > 0.0) {
        let dt = o.px - u.u_touch.xy;
        let gr = max(u.u_touch.w, 1.0);
        o.col += F.y * u.u_touch.z * exp(-dot(dt, dt) / (2.0 * gr * gr));
    }
    return o;
}
