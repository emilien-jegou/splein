// Batched solid rects: CPU-transformed screen-space verts + per-vertex color.
// Ortho uniform maps pixels -> NDC. Alpha blending handles opacity.
struct Ortho {
    w: f32,
    h: f32,
};
@group(0) @binding(0)
var<uniform> ortho: Ortho;

struct Vin {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: Vin) -> Vout {
    var out: Vout;
    let nx = (in.pos.x / ortho.w) * 2.0 - 1.0;
    let ny = 1.0 - (in.pos.y / ortho.h) * 2.0;
    out.pos = vec4<f32>(nx, ny, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    return in.color;
}
