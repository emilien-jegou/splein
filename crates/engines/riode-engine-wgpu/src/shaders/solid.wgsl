// Fullscreen blit: copies the persistent Unorm scene target onto the swapchain.
// Raw copy: shader vec4(r,g,b,a) addresses logical channels and the surface
// format describes memory layout, so no manual R/B swizzle is needed (or
// correct) for BGRA targets. The persistent target is plain Unorm holding
// straight-alpha sRGB-encoded bytes, so textureLoad returns exact stored
// values (no decode ambiguity).
//
// Transfer: on a plain Unorm swapchain this is a byte-exact copy with a
// premultiply for the compositor (vello's BlitPipeline does the same).
// On an sRGB swapchain fallback the hardware OETF encodes on store, so
// SRGB_DECODE linearizes first and the store nets identity.
override SRGB_DECODE: bool = false;

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

@group(0) @binding(0)
var src_tex: texture_2d<f32>;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = textureLoad(src_tex, vec2<i32>(pos.xy), 0);
    // Persistent target stores straight alpha; swapchain expects premultiplied RGB.
    var rgb = px.rgb * px.a;
    if (SRGB_DECODE) {
        rgb = srgb_to_linear(rgb);
    }
    return vec4<f32>(rgb, px.a);
}
