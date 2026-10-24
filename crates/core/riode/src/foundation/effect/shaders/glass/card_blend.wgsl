// Tier E — CardBlend (08.7): the CSS `mix-blend-mode` composite.
//
// Taxonomy: the "CSS mix-blend-mode on the card" row — the finished
// card (refracted backdrop and material baked in, premultiplied)
// composited against the *sharp* snapshot at this pixel through
// §"Compositing and Blending" of CSS Compositing Level 1 (same formula
// as `composite_blend.wgsl`).
//
// Deliberately NOT split and deliberately last (08.9): it needs the
// sharp snapshot and runs on the finished card. Mode 0 (`Normal`)
// returns the card byte-identical — the exact pre-blend bytes — which is
// what makes this layer free for every preset that does not ask for it.
//
// Reads: `engine.w` (the mode id rides `GlassFrameGpu.core[3]`),
// `sharp` at `px / size`.
// Writes: the fragment output.

// Card blend (CSS `mix-blend-mode` on the whole glass card): the
// finished card — refracted backdrop and material baked in,
// premultiplied — composited against the *sharp* snapshot at this
// pixel through §"Compositing and Blending" of CSS Compositing Level 1
// (same formula as `composite_blend.wgsl`). The mode id rides the
// engine header's w slot (`GlassFrameGpu.core[3]`); 0 (`Normal`)
// returns the card unchanged — the exact pre-blend bytes.
fn cardBlend(cardPrem: vec3<f32>, cardA: f32, px: vec2<f32>) -> vec4<f32> {
    let mode = i32(u.engine.w);
    if (mode == 0) {
        return vec4<f32>(cardPrem, cardA);
    }
    var cs = vec3<f32>(0.0);
    if (cardA > 0.0) {
        cs = cardPrem * (1.0 / max(cardA, 1e-6));
    }
    let bd = textureSampleLevel(sharp, samp, px / u.size.xy, 0.0);
    let ab = bd.a;
    var cb = vec3<f32>(0.0);
    if (ab > 0.0) {
        cb = bd.rgb * (1.0 / ab);
    }
    let b = blendMode(cb, cs, mode);
    let co = cardA * (1.0 - ab) * cs + cardA * ab * b + (1.0 - cardA) * ab * cb;
    let ao = cardA + ab * (1.0 - cardA);
    return vec4<f32>(co, ao);
}
