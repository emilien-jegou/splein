// Rounded-rect SDF: fill disc + stroke ring in one pass.
//
// Vertices carry device-space geometry (CPU-resolved, translation and
// uniform-scale exact; skewed transforms fall back to square solids).
// The fragment evaluates a per-corner rounded-box SDF (iq), paints the
// fill disc, then composites the stroke ring on top. `expand` places the
// ring per alignment: inside -> [-w, 0], center -> [-w/2, +w/2],
// outside -> [0, +w]. Antialiasing tracks the screen-space gradient.
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
    @location(3) fill: vec4<f32>,
    @location(4) stroke: vec4<f32>,
    @location(5) params: vec4<f32>,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) fbox: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) stroke: vec4<f32>,
    @location(4) params: vec4<f32>,
    @location(5) dpos: vec2<f32>,
};

@vertex
fn vs_main(in: Vin) -> Vout {
    var out: Vout;
    let nx = (in.pos.x / ortho.w) * 2.0 - 1.0;
    let ny = 1.0 - (in.pos.y / ortho.h) * 2.0;
    out.pos = vec4<f32>(nx, ny, 0.0, 1.0);
    out.fbox = in.fbox;
    out.radii = in.radii;
    out.fill = in.fill;
    out.stroke = in.stroke;
    out.params = in.params;
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

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    let center = in.fbox.xy + in.fbox.zw * 0.5;
    let half = in.fbox.zw * 0.5;
    let p = in.dpos - center;
    let sd = sd_round_box(p, half, in.radii);
    let w = in.params.x;
    let expand = in.params.y;
    let aa = max(fwidth(sd) * 0.75, 0.25);

    // Fill disc (sd < 0 inside), carved out where the ring paints: the
    // ring lives inside opaque fill for Inside alignment, so compositing
    // it *over* the fill would erase it (sa*(1-fa) ~= 0). Carve coverage
    // is guarded identically to sa: fill-only output is bit-identical.
    let ring_c = expand - w * 0.5;
    var ring_cov = 0.0;
    var sa = 0.0;
    if (w > 0.0 && in.stroke.a > 0.0) {
        ring_cov = 1.0 - smoothstep(w * 0.5 - aa, w * 0.5 + aa, abs(sd - ring_c));
        sa = in.stroke.a * ring_cov;
    }
    let fa = in.fill.a * (1.0 - smoothstep(-aa, aa, sd)) * (1.0 - ring_cov);
    // Straight-alpha composite: stroke over fill.
    let a = fa + sa * (1.0 - fa);
    if (a <= 0.0) { discard; }
    let rgb = (in.fill.rgb * fa + in.stroke.rgb * sa * (1.0 - fa)) / max(a, 1e-4);
    return vec4<f32>(rgb, a);
}
