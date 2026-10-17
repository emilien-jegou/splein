// Blend-mode dispatch: one entry point the glass composite calls, one
// switch over the packed wire id (`BlendMode::as_uniform_id` — the Rust
// discriminant, never renumbered). Concatenated after the separable and
// non-separable libraries, before `glass.wgsl` (pipeline.rs).
//
// Contract: `base` is the backdrop (the glass base inside the slab),
// `src` the source pass (the glow wash), both straight (non-premultiplied)
// 0..1 channels. Returns the blended colour — the caller mixes it into
// the composite by the source's alpha coverage, i.e. the CSS formula
// with an opaque backdrop: `col * (1 − a) + B(col, src) * a`.

fn blendMode(base: vec3<f32>, src: vec3<f32>, mode: i32) -> vec3<f32> {
    switch mode {
        // Separable (PDF 1.4 / CSS Compositing Level 1), wire order.
        case 1 { return blendMultiply(base, src); }
        case 2 { return blendScreen(base, src); }
        case 3 { return blendOverlay(base, src); }
        case 4 { return blendDarken(base, src); }
        case 5 { return blendLighten(base, src); }
        case 6 { return blendColorDodge(base, src); }
        case 7 { return blendColorBurn(base, src); }
        case 8 { return blendHardLight(base, src); }
        case 9 { return blendSoftLight(base, src); }
        case 10 { return blendDifference(base, src); }
        case 11 { return blendExclusion(base, src); }
        // Non-separable.
        case 12 { return blendHue(base, src); }
        case 13 { return blendSaturation(base, src); }
        case 14 { return blendColor(base, src); }
        case 15 { return blendLuminosity(base, src); }
        // Normal (0) and anything out of range: the source replaces the
        // backdrop — the pre-blend path, byte-identical through `mix`.
        default { return src; }
    }
}
