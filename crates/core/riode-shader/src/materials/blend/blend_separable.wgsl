// Separable blend modes (PDF 1.4 / CSS Compositing and Blending
// Level 1), 1..11 in wire order — each blends channel pairs
// independently. Prepended to the glass modules at pipeline build.
//
// All colours are straight (non-premultiplied) 0..1 channels. `base` is
// the backdrop, `src` the source pass.

fn blendMultiply(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return base * src;
}

fn blendScreen(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return base + src - base * src;
}

// Overlay(base, src) = HardLight(src, base): multiply where the
// *backdrop* is dark, screen where it is light.
fn blendOverlay(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return blendHardLight(src, base);
}

fn blendDarken(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return min(base, src);
}

fn blendLighten(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return max(base, src);
}

fn blendColorDodge(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    // Cb / (1 − Cs), guarded: the spec's Cb==0 → 0 and Cs==1 → 1 edge
    // cases come out of the clamps/epsilons.
    let d = base / max(vec3(1.0) - src, vec3(1e-6));
    return select(select(min(vec3(1.0), d), vec3(1.0), src >= vec3(1.0)), vec3(0.0), base <= vec3(0.0));
}

fn blendColorBurn(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    // 1 − (1 − Cb) / Cs, guarded the same way.
    let d = vec3(1.0) - min(vec3(1.0), (vec3(1.0) - base) / max(src, vec3(1e-6)));
    return select(select(d, vec3(0.0), src <= vec3(0.0)), vec3(1.0), base >= vec3(1.0));
}

// HardLight(base, src) = Overlay(src, base): multiply where the
// *source* is dark, screen where it is light.
fn blendHardLight(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return select(
        blendScreen(base, 2.0 * src - 1.0),
        blendMultiply(base, 2.0 * src),
        src <= vec3(0.5),
    );
}

fn blendSoftLight(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    // W3C D(B): the cubic ramp below 0.25, √B above — continuous at the
    // knot (both give 0.5 at B = 0.25); NOT the B² misremembering.
    let d = select(sqrt(base), ((16.0 * base - 12.0) * base + 4.0) * base, base <= vec3(0.25));
    return select(
        base + (2.0 * src - 1.0) * (d - base),
        base - (1.0 - 2.0 * src) * base * (1.0 - base),
        src <= vec3(0.5),
    );
}

fn blendDifference(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return abs(base - src);
}

fn blendExclusion(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return base + src - 2.0 * base * src;
}
