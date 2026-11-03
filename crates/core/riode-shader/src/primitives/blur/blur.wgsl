// One separable pass of an element-filter Gaussian blur (CSS `filter:
// blur()`), over a surface-sized layer offscreen.
//
// Unfiltered `textureLoad` gives nearest sampling, so taps must cover
// every texel across ±3σ: sparse spacing (e.g. 0.75σ) lands as discrete
// ghost copies of the sharp source and edges repeat as visible bands.
// Weights are in-shader Gaussians normalized by the tap sum; the
// half-count is capped so extreme σ stays bounded (the step grows past
// σ ≈ 40 texels — ghosting there is imperceptible at that width).
// Loads clamp at the edges — no sampler needed (the blit precedent).
// The two passes (dir = X then Y) compose the 2D Gaussian; blend is
// disabled because the destination is cleared by its load op every use.

struct BlurParams {
    sigma: f32,
    dir_x: f32,
    dir_y: f32,
    pad: f32,
};
@group(0) @binding(0)
var src_tex: texture_2d<f32>;
@group(0) @binding(1)
var<uniform> params: BlurParams;

@vertex
fn vs_main(@builtin(vertex_index) ix: u32) -> @builtin(position) vec4<f32> {
    var p = vec2<f32>(-1.0, 1.0);
    switch ix {
        case 1u { p = vec2<f32>(-1.0, -1.0); }
        case 2u, 4u { p = vec2<f32>(1.0, -1.0); }
        case 5u { p = vec2<f32>(1.0, 1.0); }
        default {}
    }
    return vec4<f32>(p, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(src_tex, 0));
    let base = vec2<i32>(pos.xy);
    // Guard σ <= 0 for the weight denominator (such passes are skipped
    // CPU-side anyway; this keeps the shader NaN-free).
    let sigma = max(params.sigma, 1e-4);
    let dir = vec2<f32>(params.dir_x, params.dir_y);
    let half_n = min(ceil(3.0 * sigma), 120.0);
    let step = (3.0 * sigma) / half_n;
    let n = i32(half_n);
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var k = -n; k <= n; k = k + 1) {
        let off = f32(k) * step;
        let c = clamp(
            base + vec2<i32>(round(dir * off)),
            vec2<i32>(0),
            size - vec2<i32>(1),
        );
        let w = exp(-0.5 * (off * off) / (sigma * sigma));
        acc = acc + textureLoad(src_tex, c, 0) * w;
        wsum = wsum + w;
    }
    return acc / wsum;
}
