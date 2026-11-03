// Single responsibility: Shader compilation and uniform layout resolution for surfaces, overlays, and effects.

use riode::foundation::{BlendMode, Geo, LayerPaint, Material, Shape, Stage};
use crate::primitives::blend::BLEND_LIBRARY_WGSL;

pub struct ComposedMaterialShader {
    pub wgsl: String,
    pub total_uniform_size: usize,
    pub requires_backdrop: bool,
}

pub fn compose_material_shader(paint: &LayerPaint) -> ComposedMaterialShader {
    compose_surface_shader(paint.shape.as_ref(), paint.material.as_ref())
}

/// Shape stages are prepended before `chain` — the surface fill — in one
/// shader. Both public composers funnel through here so the bundle is only
/// taken apart at the point where the ordering actually matters.
fn compose_surface_shader(
    shape: Option<&Shape>,
    chain: Option<&Material>,
) -> ComposedMaterialShader {
    let mut stages: Vec<&Box<dyn Stage>> = Vec::new();
    if let Some(sh) = shape {
        stages.extend(sh.stages());
    }
    if let Some(mat) = chain {
        stages.extend(mat.stages());
    }

    let has_blends = stages.iter().any(|s| !s.blend_mode().is_normal());
    let mut wgsl = String::with_capacity(2048);

    if has_blends {
        wgsl.push_str(BLEND_LIBRARY_WGSL);
        wgsl.push_str("\n");
    }

    // 1. Unified ABI: FragmentPayload & ElementGeo
    wgsl.push_str(r#"
struct FragmentPayload {
    color: vec4<f32>,     // Accumulated premultiplied linear color
    dist: f32,            // Signed distance to element edge (SDF)
    pos: vec2<f32>,       // Device pixel coordinate [0..width, 0..height]
    uv: vec2<f32>,        // Normalized box coordinate [0.0..1.0]
    normal: vec3<f32>,    // Surface normal vector (derived from SDF gradient / lens)
    backdrop: vec4<f32>,  // Read-only snapshot of underlying surface
};

struct ElementGeo {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    surface_w: f32,
    surface_h: f32,
    dpr: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> geo: ElementGeo;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) ix: u32) -> VertexOutput {
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0)
    );
    let u = uvs[ix];
    let px = geo.x + u.x * geo.width;
    let py = geo.y + u.y * geo.height;

    var out: VertexOutput;
    out.position = vec4<f32>(
        (px / geo.surface_w) * 2.0 - 1.0,
        1.0 - (py / geo.surface_h) * 2.0,
        0.0,
        1.0
    );
    out.uv = u;
    return out;
}
"#);

    // 2. Stage function declarations
    let mut total_uniform_size = 0;
    let mut requires_backdrop = false;

    for (idx, stage) in stages.iter().enumerate() {
        let size = stage.uniform_size();
        let aligned_size = (size + 15) & !15;
        total_uniform_size += aligned_size;
        if stage.requires_backdrop() {
            requires_backdrop = true;
        }

        wgsl.push_str(&format!("\n// --- Stage {} ---\n", idx));
        let src = stage
            .wgsl_source()
            .replace("fn stage_apply", &format!("fn stage_{}_apply", idx));
        wgsl.push_str(&src);
    }

    if total_uniform_size > 0 {
        let num_vec4 = total_uniform_size / 16;
        wgsl.push_str(&format!(
            "\nstruct MaterialUniforms {{\n    data: array<vec4<f32>, {}>,\n}};\n@group(1) @binding(0) var<uniform> uniforms: MaterialUniforms;\n",
            num_vec4
        ));
    }

    // 3. Fragment entry point with stage blending
    wgsl.push_str(r#"
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var payload: FragmentPayload;
    payload.color = vec4<f32>(0.0);
    payload.pos = in.uv * vec2<f32>(geo.width, geo.height);
    payload.uv = in.uv;
    payload.normal = vec3<f32>(0.0, 0.0, 1.0);
    payload.backdrop = vec4<f32>(0.0);

    let half_dim = vec2<f32>(geo.width, geo.height) * 0.5;
    let p = payload.pos - half_dim;
    let d = abs(p) - half_dim;
    payload.dist = length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
"#);

    let mut vec4_offset = 0;
    for (idx, stage) in stages.iter().enumerate() {
        let size = stage.uniform_size();
        let aligned_size = (size + 15) & !15;
        let mode = stage.blend_mode();

        if !mode.is_normal() {
            wgsl.push_str("    let prev_color = payload.color;\n");
        }

        if size > 0 {
            wgsl.push_str(&format!(
                "    stage_{idx}_apply(&payload, uniforms.data[{vec4_offset}]);\n"
            ));
            vec4_offset += aligned_size / 16;
        } else {
            wgsl.push_str(&format!("    stage_{idx}_apply(&payload);\n"));
        }

        if !mode.is_normal() {
            let mode_id = mode as u8;
            wgsl.push_str(&format!(
                r#"    if (payload.color.a > 0.0 && prev_color.a > 0.0) {{
        let cb = prev_color.rgb / prev_color.a;
        let cs = payload.color.rgb / payload.color.a;
        let blended = blendMode(cb, cs, {mode_id});
        let a_src = payload.color.a;
        let a_dst = prev_color.a;
        let co = a_src * (1.0 - a_dst) * cs + a_src * a_dst * blended + (1.0 - a_src) * a_dst * cb;
        let ao = a_src + a_dst * (1.0 - a_src);
        payload.color = vec4<f32>(co, ao);
    }}
"#
            ));
        }
    }

    wgsl.push_str(r#"
    return payload.color;
}
"#);

    ComposedMaterialShader {
        wgsl,
        total_uniform_size,
        requires_backdrop,
    }
}

pub fn write_material_uniforms(
    paint: &LayerPaint,
    geo: &Geo,
    buffer: &mut [u8],
) -> usize {
    write_surface_uniforms(paint.shape.as_ref(), paint.material.as_ref(), geo, buffer)
}

fn write_surface_uniforms(
    shape: Option<&Shape>,
    chain: Option<&Material>,
    geo: &Geo,
    buffer: &mut [u8],
) -> usize {
    let mut offset = 0;
    let mut write_stage = |stage: &dyn Stage| {
        let size = stage.uniform_size();
        if size > 0 {
            stage.write_uniforms(geo, &mut buffer[offset..offset + size]);
            let aligned = (size + 15) & !15;
            offset += aligned;
        }
    };

    if let Some(sh) = shape {
        for s in sh.stages() {
            write_stage(&**s);
        }
    }
    if let Some(mat) = chain {
        for s in mat.stages() {
            write_stage(&**s);
        }
    }

    offset
}

pub fn compose_overlay_shader(paint: &LayerPaint) -> ComposedMaterialShader {
    let mat = paint.overlay.as_ref().map(|o| o.to_material());
    compose_surface_shader(paint.shape.as_ref(), mat.as_ref())
}

pub fn write_overlay_uniforms(paint: &LayerPaint, geo: &Geo, buffer: &mut [u8]) -> usize {
    let mat = paint.overlay.as_ref().map(|o| o.to_material());
    write_surface_uniforms(paint.shape.as_ref(), mat.as_ref(), geo, buffer)
}

pub struct ComposedEffectShader {
    pub wgsl: String,
    pub total_uniform_size: usize,
    pub requires_backdrop: bool,
    pub blend_mode: BlendMode,
}

pub fn compose_effect_shader(paint: &LayerPaint) -> ComposedEffectShader {
    let effect = paint.effect.as_ref();
    let stages = effect.map(|e| e.stages()).unwrap_or(&[]);
    let blend_mode = effect.map(|e| e.composite_blend()).unwrap_or(BlendMode::Normal);
    let requires_backdrop = effect.is_some_and(|e| e.requires_backdrop()) || !blend_mode.is_normal();

    let mut wgsl = String::with_capacity(2048);

    if requires_backdrop {
        wgsl.push_str(BLEND_LIBRARY_WGSL);
        wgsl.push_str("\n");
    }

    wgsl.push_str(r#"
struct Ortho {
    w: f32,
    h: f32,
};
struct ElementGeo {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    surface_w: f32,
    surface_h: f32,
    dpr: f32,
    time: f32,
};

@group(0) @binding(0) var<uniform> ortho: Ortho;
@group(0) @binding(1) var<uniform> geo: ElementGeo;

@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var dst_tex: texture_2d<f32>;
@group(1) @binding(2) var effect_sampler: sampler;
"#);

    let mut total_uniform_size = 0;
    for (idx, stage) in stages.iter().enumerate() {
        let size = stage.uniform_size();
        let aligned_size = (size + 15) & !15;
        total_uniform_size += aligned_size;

        let src = stage.wgsl_source();
        if !src.is_empty() {
            wgsl.push_str(&format!("\n// --- Effect Stage {} ---\n", idx));
            let replaced = src.replace("fn effect_apply", &format!("fn effect_{}_apply", idx));
            wgsl.push_str(&replaced);
        }
    }

    if total_uniform_size > 0 {
        let num_vec4 = total_uniform_size / 16;
        wgsl.push_str(&format!(
            "\nstruct EffectUniforms {{\n    data: array<vec4<f32>, {}>,\n}};\n@group(1) @binding(3) var<uniform> uniforms: EffectUniforms;\n",
            num_vec4
        ));
    }

    wgsl.push_str(r#"
struct Vin {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(in: Vin) -> Vout {
    var out: Vout;
    let nx = (in.pos.x / ortho.w) * 2.0 - 1.0;
    let ny = 1.0 - (in.pos.y / ortho.h) * 2.0;
    out.pos = vec4<f32>(nx, ny, 0.0, 1.0);
    out.color = in.color;
    out.uv = in.pos / vec2<f32>(ortho.w, ortho.h);
    return out;
}

@fragment
fn fs_main(in: Vout) -> @location(0) vec4<f32> {
    var sample_pos = in.pos.xy;
    let size = vec2<f32>(textureDimensions(src_tex, 0));
"#);

    let mut vec4_offset = 0;
    for (idx, stage) in stages.iter().enumerate() {
        let size = stage.uniform_size();
        let aligned_size = (size + 15) & !15;
        let src = stage.wgsl_source();
        if src.contains("effect_apply_uv") {
            if size > 0 {
                wgsl.push_str(&format!(
                    "    sample_pos = effect_{idx}_apply_uv(sample_pos, geo, uniforms.data[{vec4_offset}]);\n"
                ));
            } else {
                wgsl.push_str(&format!("    sample_pos = effect_{idx}_apply_uv(sample_pos, geo);\n"));
            }
        }
        if size > 0 {
            vec4_offset += aligned_size / 16;
        }
    }

    wgsl.push_str(r#"
    let uv_sample = sample_pos / size;
    var src_col = textureSampleLevel(src_tex, effect_sampler, uv_sample, 0.0);
"#);

    vec4_offset = 0;
    for (idx, stage) in stages.iter().enumerate() {
        let size = stage.uniform_size();
        let aligned_size = (size + 15) & !15;
        let src = stage.wgsl_source();
        if src.contains("effect_apply_color") {
            if size > 0 {
                wgsl.push_str(&format!(
                    "    src_col = effect_{idx}_apply_color(src_col, sample_pos, geo, uniforms.data[{vec4_offset}]);\n"
                ));
            } else {
                wgsl.push_str(&format!(
                    "    src_col = effect_{idx}_apply_color(src_col, sample_pos, geo);\n"
                ));
            }
        }
        if size > 0 {
            vec4_offset += aligned_size / 16;
        }
    }

    if requires_backdrop && !blend_mode.is_normal() {
        let mode_id = blend_mode as u8;
        wgsl.push_str(&format!(
            r#"
    let bd = textureLoad(dst_tex, vec2<i32>(floor(in.pos.xy)), 0);
    if (src_col.a <= 0.0) {{ return vec4<f32>(0.0); }}
    if (bd.a <= 0.0) {{ return src_col * in.color; }}
    let cb = bd.rgb / bd.a;
    let cs = src_col.rgb / src_col.a;
    let blended = blendMode(cb, cs, {mode_id});
    let a_src = src_col.a * in.color.a;
    let a_dst = bd.a;
    let co = a_src * (1.0 - a_dst) * cs + a_src * a_dst * blended + (1.0 - a_src) * a_dst * cb;
    let ao = a_src + a_dst * (1.0 - a_src);
    return vec4<f32>(co, ao);
"#
        ));
    } else {
        wgsl.push_str(r#"
    return src_col * in.color;
"#);
    }

    wgsl.push_str("}\n");

    ComposedEffectShader {
        wgsl,
        total_uniform_size,
        requires_backdrop,
        blend_mode,
    }
}

pub fn write_effect_uniforms(paint: &LayerPaint, geo: &Geo, buffer: &mut [u8]) -> usize {
    let mut offset = 0;
    if let Some(eff) = paint.effect.as_ref() {
        for stage in eff.stages() {
            let size = stage.uniform_size();
            if size > 0 {
                stage.write_uniforms(geo, &mut buffer[offset..offset + size]);
                let aligned = (size + 15) & !15;
                offset += aligned;
            }
        }
    }
    offset
}
