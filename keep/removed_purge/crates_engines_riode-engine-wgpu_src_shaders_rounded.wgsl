// Rounded-rect SDF double-wall: fill + border in one pass, encoded-space.
//
// Vertices carry device-space geometry (CPU-resolved, translation and
// uniform-scale exact; skewed transforms fall back to square solids).
// StrokeAlign is resolved CPU-side into two walls: `fbox`/`radii` is the
// outer wall (fill + border), `ibox`/`iradii` the inner wall (fill only).
// The fragment paints fill inside the inner wall and the border in the
// band between walls (gpui-style double-wall). Colors arrive
// sRGB-encoded and composite as-is on the plain Unorm target
// (vello-like, no transfer handling).
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
    @location(3) ibox: vec4<f32>,
    @location(4) iradii: vec4<f32>,
    @location(5) fill: vec4<f32>,
    @location(6) stroke: vec4<f32>,
    @location(7) grad: vec4<f32>,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) fbox: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) ibox: vec4<f32>,
    @location(3) iradii: vec4<f32>,
    @location(4) fill: vec4<f32>,
    @location(5) stroke: vec4<f32>,
    @location(6) grad: vec4<f32>,
    @location(7) dpos: vec2<f32>,
};

@vertex
fn vs_main(in: Vin) -> Vout {
    var out: Vout;
    let nx = (in.pos.x / ortho.w) * 2.0 - 1.0;
    let ny = 1.0 - (in.pos.y / ortho.h) * 2.0;
    out.pos = vec4<f32>(nx, ny, 0.0, 1.0);
    out.fbox = in.fbox;
    out.radii = in.radii;
    out.ibox = in.ibox;
    out.iradii = in.iradii;
    out.fill = in.fill;
    out.stroke = in.stroke;
    out.grad = in.grad;
    out.dpos = in.pos;
    return out;
}

fn sd_round_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    // r = (top_left, top_right, bottom_right, bottom_left) in screen
    // coords (y down). abs() folds to quadrants; pick the matching corner.
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

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// Oklab (Bjorn Ottosson) inverse: lab -> linear sRGB. Cubes preserve
// sign, so slightly-negative l_/m_/s_ intermediates stay exact.
fn oklab_to_linear_srgb(lab: vec3<f32>) -> vec3<f32> {
    let l_ = lab.x + 0.3963377774 * lab.y + 0.2158037573 * lab.z;
    let m_ = lab.x - 0.1055613458 * lab.y - 0.0638541728 * lab.z;
    let s_ = lab.x - 0.0894841775 * lab.y - 1.2914855480 * lab.z;
    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;
    return vec3<f32>(
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    );
}

// Ordered Bayer 4x4, normalized to [0, 1). Dither amplitude stays
// 1/255: imperceptible, but it breaks 8-bit gradient banding.
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

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    let center = in.fbox.xy + in.fbox.zw * 0.5;
    let half = in.fbox.zw * 0.5;
    let p = in.dpos - center;
    let sd = sd_round_box(p, half, in.radii);
    let icenter = in.ibox.xy + in.ibox.zw * 0.5;
    let ihalf = max(in.ibox.zw * 0.5, vec2<f32>(0.0));
    let ip = in.dpos - icenter;
    let isd = sd_round_box(ip, ihalf, in.iradii);
    let aa = max(fwidth(sd) * 0.75, 0.25);

    // Double-wall coverage: fill inside the inner wall, border in the
    // band between walls. When no border is emitted the CPU sets
    // ibox == fbox / iradii == radii, so the fill term is bit-identical
    // to a plain SDF disc.
    let oc = 1.0 - smoothstep(-aa, aa, sd);
    let ic = 1.0 - smoothstep(-aa, aa, isd);
    var fa = in.fill.a * ic;
    var sa = in.stroke.a * oc * (1.0 - ic);
    var fill_enc = in.fill.rgb;
    // Gradient mode (grad.xy is the unit axis; solid fills leave it
    // zero): fill/stroke hold Oklab + alpha for the two stops; t is the
    // stop projection onto the axis from the outer-box origin. Alpha
    // interpolates with the colors (CSS parity). Oklab inverts to linear
    // and re-encodes to sRGB; ordered dither applies in encoded space.
    let is_grad = in.grad.x != 0.0 || in.grad.y != 0.0;
    if (is_grad) {
        let tp = in.dpos - in.fbox.xy;
        let t = clamp(
            (dot(tp, in.grad.xy) - in.grad.z) / max(in.grad.w - in.grad.z, 1e-6),
            0.0,
            1.0,
        );
        let lab = mix(in.fill.rgb, in.stroke.rgb, t);
        var col = linear_to_srgb(oklab_to_linear_srgb(lab));
        let dither = (bayer_4x4(vec2<u32>(floor(in.dpos))) - 0.5) / 255.0;
        col = clamp(col + vec3<f32>(dither), vec3<f32>(0.0), vec3<f32>(1.0));
        fill_enc = col;
        fa = mix(in.fill.a, in.stroke.a, t) * ic;
        sa = 0.0; // stroke slot carries stop 1, not a border
    }
    // Straight-alpha composite in encoded space: border over fill.
    let a = fa + sa * (1.0 - fa);
    if (a <= 0.0) { discard; }
    let frgb = fill_enc * fa;
    let brgb = in.stroke.rgb * sa * (1.0 - fa);
    let rgb = (frgb + brgb) / max(a, 1e-4);
    return vec4<f32>(rgb, a);
}
