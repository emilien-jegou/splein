// Fullscreen blit: samples the persistent Rgba8Unorm target onto the swapchain.
// Raw copy: shader vec4(r,g,b,a) addresses logical channels and the surface
// format describes memory layout, so no manual R/B swizzle is needed (or
// correct) for BGRA targets.
//
// Transfer: scene bytes are already sRGB-encoded. On a plain Unorm surface
// the copy below is exact (no hardware conversion). Only when forced onto
// an sRGB surface (see context format fallback), SRGB_DECODE applies the
// EOTF first so the hardware OETF on store nets back to identity instead
// of double-encoding.
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

fn srgb_eotf(c: vec3<f32>) -> vec3<f32> {
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
        rgb = srgb_eotf(rgb);
    }
    return vec4<f32>(rgb, px.a);
}
