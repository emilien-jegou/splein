// Non-separable blend modes (PDF 1.4 / CSS Compositing and Blending
// Level 1): Hue, Saturation, Color, Luminosity — plus the Lum/SetLum/
// SetSat/ClipColor colour-space helpers they share. Prepended to
// `glass.wgsl` at pipeline build (WGSL has no include mechanism).
//
// All colours are straight (non-premultiplied) 0..1 channels.

// Rec.601-ish luminance used by the spec's colour-space helpers.
fn blendLum(c: vec3<f32>) -> f32 {
    return 0.3 * c.r + 0.59 * c.g + 0.11 * c.b;
}

// Channel spread: max − min.
fn blendSat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

// Rescale mid/max toward the target spread, min to 0, preserving which
// original channel held each rank.
fn blendSetSat(c: vec3<f32>, s: f32) -> vec3<f32> {
    var v = c;
    // Rank the three channels (indices into the vec3).
    var i_min = 0u;
    var i_mid = 1u;
    var i_max = 2u;
    if (v[i_mid] < v[i_min]) {
        let t = i_min;
        i_min = i_mid;
        i_mid = t;
    }
    if (v[i_max] < v[i_min]) {
        let t = i_min;
        i_min = i_max;
        i_max = t;
    }
    if (v[i_max] < v[i_mid]) {
        let t = i_mid;
        i_mid = i_max;
        i_max = t;
    }
    let mn = v[i_min];
    let md = v[i_mid];
    let mx = v[i_max];
    if (mx > mn) {
        v[i_mid] = (md - mn) * s / (mx - mn);
        v[i_max] = s;
    } else {
        v[i_mid] = 0.0;
        v[i_max] = 0.0;
    }
    v[i_min] = 0.0;
    return v;
}

// Keep the colour in gamut after a luminance shift: pull the dark end up
// if any channel is negative, pull the bright end down if any exceeds 1.
fn blendClipColor(c: vec3<f32>) -> vec3<f32> {
    let l = blendLum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    var out = c;
    if (n < 0.0) {
        out = vec3(l) + (c - vec3(l)) * l / max(l - n, 1e-6);
    }
    if (x > 1.0) {
        out = vec3(l) + (out - vec3(l)) * (1.0 - l) / max(x - l, 1e-6);
    }
    return out;
}

// Shift the colour's luminance to `l`, staying in gamut.
fn blendSetLum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return blendClipColor(c + vec3(l - blendLum(c)));
}

// Hue: source hue over the backdrop's saturation and luminance.
fn blendHue(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return blendSetLum(blendSetSat(src, blendSat(base)), blendLum(base));
}

// Saturation: source saturation over the backdrop's hue and luminance.
fn blendSaturation(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return blendSetLum(blendSetSat(base, blendSat(src)), blendLum(base));
}

// Color: source hue and saturation over the backdrop's luminance.
fn blendColor(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return blendSetLum(src, blendLum(base));
}

// Luminosity: source luminance over the backdrop's hue and saturation.
fn blendLuminosity(base: vec3<f32>, src: vec3<f32>) -> vec3<f32> {
    return blendSetLum(base, blendLum(src));
}
