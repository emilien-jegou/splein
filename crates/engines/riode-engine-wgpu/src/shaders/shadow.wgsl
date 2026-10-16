// Shadow SDF box: verbatim port of gpui's analytic Gaussian
// (crates/gpui_wgpu/src/shaders.wgsl, fs_shadow / blur_along_x /
// gaussian / erf — Evan Wallace's separable-convolution technique).
// Exact erf convolution along x at 4 Gaussian-weighted row samples
// along y, rows clamped to +-3 sigma and to rows whose profile can
// reach the point. The CPU pads the quad by >= 3*sigma (emit_shadow).
//
// One quad per shadow layer; the CPU bakes the layer alpha (light
// intensity * color.a * layer opacity * node opacity) into color.a and
// the sigma into params.x. Ordered Bayer dither on the final alpha
// breaks banding of very soft falloffs (our addition; gpui lacks it).
struct Ortho {
    w: f32,
    h: f32,
};
@group(0) @binding(0)
var<uniform> ortho: Ortho;

struct Vin {
    @location(0) pos: vec2<f32>,
    @location(1) fbox: vec4<f32>,
    @location(2) radii: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) params: vec4<f32>,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) fbox: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) dpos: vec2<f32>,
};

@vertex
fn vs_main(in: Vin) -> Vout {
    var out: Vout;
    let nx = (in.pos.x / ortho.w) * 2.0 - 1.0;
    let ny = 1.0 - (in.pos.y / ortho.h) * 2.0;
    out.pos = vec4<f32>(nx, ny, 0.0, 1.0);
    out.fbox = in.fbox;
    out.radii = in.radii;
    out.color = in.color;
    out.params = in.params;
    out.dpos = in.pos;
    return out;
}

fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    // r = (top_left, top_right, bottom_right, bottom_left), y down.
    var rad = r.x;
    if (p.x >= 0.0 && p.y >= 0.0) {
        rad = r.z;
    } else if (p.x >= 0.0) {
        rad = r.y;
    } else if (p.y >= 0.0) {
        rad = r.w;
    }
    let q = abs(p) - b + rad;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - rad;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// Ordered Bayer 4x4, normalized to [0, 1).
fn bayer_4x4(p: vec2<u32>) -> f32 {
    var m = array<f32, 16>(
        0.0, 8.0, 2.0, 10.0,
        12.0, 4.0, 14.0, 6.0,
        3.0, 11.0, 1.0, 9.0,
        15.0, 7.0, 13.0, 5.0
    );
    let x = p.x % 4u;
    let y = p.y % 4u;
    return (m[y * 4u + x] + 0.5) / 16.0;
}

// ---- gpui verbatim (fs_shadow helpers) ----

const M_PI_F: f32 = 3.1415926;

// A standard gaussian function, used for weighting samples.
fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (sqrt(2.0 * M_PI_F) * sigma);
}

// This approximates the error function, needed for the gaussian integral.
fn erf(v: vec2<f32>) -> vec2<f32> {
    let s = sign(v);
    let a = abs(v);
    let r1 = 1.0 + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
    let r2 = r1 * r1;
    return s - s / (r2 * r2);
}

fn blur_along_x(x: f32, y: f32, sigma: f32, corner: f32, half_size: vec2<f32>) -> f32 {
    let delta = min(half_size.y - corner - abs(y), 0.0);
    let curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let integral = 0.5 + 0.5 * erf((x + vec2<f32>(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    let half_size = in.fbox.zw * 0.5;
    let center = in.fbox.xy + half_size;
    let center_to_point = in.dpos - center;
    let sigma = in.params.x;

    // Radius of the nearest corner (gpui pick_corner_radius; our radii
    // are (top_left, top_right, bottom_right, bottom_left)).
    var corner_radius: f32;
    if (center_to_point.x < 0.0) {
        if (center_to_point.y < 0.0) {
            corner_radius = in.radii.x;
        } else {
            corner_radius = in.radii.w;
        }
    } else {
        if (center_to_point.y < 0.0) {
            corner_radius = in.radii.y;
        } else {
            corner_radius = in.radii.z;
        }
    }

    var alpha: f32;
    if (sigma <= 0.0) {
        let distance = sd_round_box(center_to_point, half_size, in.radii);
        alpha = clamp(0.5 - distance, 0.0, 1.0);
    } else {
        // The signal is only non-zero in a limited range, so don't
        // waste samples.
        let low = center_to_point.y - half_size.y;
        let high = center_to_point.y + half_size.y;
        let start = clamp(-3.0 * sigma, low, high);
        let end = clamp(3.0 * sigma, low, high);

        // Accumulate samples (we can get away with surprisingly few
        // samples).
        let step = (end - start) / 4.0;
        var y = start + step * 0.5;
        alpha = 0.0;
        for (var i = 0u; i < 4u; i = i + 1u) {
            let blur = blur_along_x(
                center_to_point.x,
                center_to_point.y - y,
                sigma,
                corner_radius,
                half_size
            );
            alpha = alpha + blur * gaussian(y, sigma) * step;
            y = y + step;
        }
    }

    let dither = (bayer_4x4(vec2<u32>(floor(in.dpos))) - 0.5) / 255.0;
    let a = clamp(alpha * in.color.a + dither, 0.0, 1.0);
    if (a <= 0.0) {
        discard;
    }
    return vec4<f32>(srgb_to_linear(in.color.rgb), a);
}
