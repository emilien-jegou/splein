// Fullscreen blit: samples the persistent sRGB scene target onto the swapchain.
// Raw copy: shader vec4(r,g,b,a) addresses logical channels and the surface
// format describes memory layout, so no manual R/B swizzle is needed (or
// correct) for BGRA targets.
//
// Transfer: the persistent target is *Srgb — textureLoad returns LINEAR
// values (hardware decodes on read; the scene pass blended in linear and
// the hardware re-encoded on store). Premultiply therefore happens in
// linear. On an sRGB swapchain the hardware OETF encodes on store; on a
// plain Unorm swapchain SRGB_ENCODE applies the OETF here so the bytes
// reaching the compositor are sRGB-encoded either way.
override SRGB_ENCODE: bool = false;

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

fn srgb_oetf(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = textureLoad(src_tex, vec2<i32>(pos.xy), 0);
    // Persistent target stores straight alpha; swapchain expects premultiplied RGB.
    var rgb = px.rgb * px.a;
    if (SRGB_ENCODE) {
        rgb = srgb_oetf(rgb);
    }
    return vec4<f32>(rgb, px.a);
}
