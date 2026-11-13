// Fullscreen blit: copies the persistent scene target onto the swapchain.
// Raw copy: shader vec4(r,g,b,a) addresses logical channels and the surface
// format describes memory layout, so no manual R/B swizzle is needed (or
// correct) for BGRA targets. The persistent target is sRGB-tagged holding
// straight-alpha LINEAR values, so textureLoad returns decoded linear RGB
// (Vulkan/WebGPU decode sRGB formats on every sampled read, fetch included)
// and alpha untouched.
//
// Transfer: this is the one encode point of the pipeline. The shader
// premultiplies (persistent stores straight alpha; the swapchain/compositor
// expects premultiplied RGB) and encodes linear -> sRGB on a plain Unorm
// swapchain. On an sRGB swapchain fallback the hardware OETF encodes on
// store, so SRGB_ENCODE stays off and the store nets the same encode.
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

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = vec3<f32>(1.055) * pow(c, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = textureLoad(src_tex, vec2<i32>(pos.xy), 0);
    // Persistent target stores straight linear alpha; swapchain expects
    // premultiplied encoded RGB.
    var rgb = px.rgb * px.a;
    if (SRGB_ENCODE) {
        rgb = linear_to_srgb(rgb);
    }
    return vec4<f32>(rgb, px.a);
}
