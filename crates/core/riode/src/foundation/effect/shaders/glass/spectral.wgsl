// Tier B4 — Spectral (08.5): chromatic dispersion of the lens sample.
//
// Taxonomy: L4 dispersion. Spectral refraction is independent of the
// lens material: the lens supplies one neutral refraction offset; this
// optional sampler widens that offset once per wavelength and
// recombines the taps. The rainbow therefore modulates refraction, but
// is not a glass-material stage — which is why it is its own effect and
// not a knob inside `Refract`.
//
// Deliberately NOT split (08.9): it needs one accumulator across
// wavelengths; per-wavelength effects are meaningless.
//
// Reads: `sample_point`, `normal`, `lensAmt`, `vv`, `base`, `col`;
// `u_var[32]` (Q: spectral base IOR, spread, reach — spread/reach 0 is
// an exact no-op), `px` (hash phase).
// Writes: `col`.

fn spectralLobe(x: f32, m: f32, s1: f32, s2: f32) -> f32 {
    let t = (x - m) * select(s1, s2, x < m);
    return exp(-0.5 * t * t);
}

fn spectralWeight(lambda: f32) -> vec3<f32> {
    let l = lambda;
    let x = 0.362 * spectralLobe(l, 442.0, 0.0624, 0.0374)
        + 1.056 * spectralLobe(l, 599.8, 0.0264, 0.0323)
        - 0.065 * spectralLobe(l, 501.1, 0.049, 0.0382);
    let y = 0.821 * spectralLobe(l, 568.8, 0.0213, 0.0247)
        + 0.286 * spectralLobe(l, 530.9, 0.0613, 0.0322);
    let z = 1.217 * spectralLobe(l, 437.0, 0.0845, 0.0278)
        + 0.681 * spectralLobe(l, 459.0, 0.0385, 0.0725);
    return max(
        vec3<f32>(
            3.2406 * x - 1.5372 * y - 0.4986 * z,
            -0.9689 * x + 1.8752 * y + 0.0415 * z,
            0.0557 * x - 0.2040 * y + 1.0570 * z
        ),
        vec3<f32>(0.0)
    );
}

fn spectralPhase(p: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

// The CIE spectrum weights are fitted in linear light, matching the
// faithful reference. Convert this display-referred sampler's taps to
// linear before recombination, then return the blended refraction to the
// pipeline's encoded colour. Monochrome glass above and below is untouched.
fn spectralToLinear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn spectralFromLinear(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

/// Spectral refraction: widens the lens sample once per wavelength.
/// Spread/reach 0 is an exact no-op.
fn stageSpectral(g: GlassCtx) -> GlassCtx {
    var o = g;
    // Spectral refraction widens the lens sample once per wavelength.
    // The spectral taps carry the core channel's full frost offset, and
    // the spectrum weights are normalized, so spread/reach 0 is an exact
    // no-op: the single-tap frosted path above is used unchanged, with no
    // extra backdrop taps. A grey backdrop stays grey because the offset
    // is measured after the wide mix, not before it.
    let Q = u.u_var[32];
    if (Q.y != 0.0 && Q.z != 0.0) {
        let baseLinear = spectralToLinear(o.base);
        let coreOff = spectralToLinear(o.col) - baseLinear;
        let iorBase = max(Q.x, 1.0);
        let bendBase = max(1.0 - 1.0 / iorBase, 1e-4);
        let reach = max(Q.z, 0.0);
        let phase = spectralPhase(o.px);
        var acc = vec3<f32>(0.0);
        var wsum = vec3<f32>(0.0);
        for (var k = 0; k < 8; k = k + 1) {
            let frac = (f32(k) + phase) / 8.0;
            let lambda = 405.0 + 290.0 * frac;
            let um = lambda * 0.001;
            let iorWave = iorBase + Q.y * (1.0 / (um * um) - 2.8962) / 1.91;
            let bendWave = 1.0 - 1.0 / max(iorWave, 1.0);
            let spoke = o.normal * o.lensAmt * (reach * (bendWave / bendBase - 1.0));
            let w = spectralWeight(lambda);
            acc = acc + (spectralToLinear(tex(o.sample_point + spoke).rgb) + coreOff) * w;
            wsum = wsum + w;
        }
        o.col = mix(o.col, spectralFromLinear(acc / max(wsum, vec3<f32>(1e-4))), o.vv);
    }
    return o;
}
