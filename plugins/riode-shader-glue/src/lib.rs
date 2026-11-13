// Single responsibility: WGSL -> GLSL ES 3.00 transpile for the
// riode-glass compositor plugin.
//
// The plugin (C++/GL) receives complete wire modules (effect ABI header
// + the custom `apply` module, composed client-side) and needs GLSL it
// can glCompileShader. This crate does the conversion with naga and
// exposes it over a plain C API, mirroring the item-helper pattern: the
// cdylib is embedded in riode-glass.so, written to a memfd and
// dlopen'd with RTLD_NODELETE at PLUGIN_INIT. The plugin stays pure
// C++/GL; naga never enters its build.
//
// Transpile contract:
// - input = the complete WGSL module as it rode the wire (the client
//   prepends the effect ABI header; see EFFECT_ABI_WGSL), plus this
//   crate's entry scaffold appended verbatim;
// - output = one GLSL ES 3.00 fragment shader, entry `fs_main`,
//   raw gl_FragCoord (no Y adjust — the plugin's captures are already
//   GL-row oriented, like GlassOptics);
// - failure = a human-readable reason; the plugin drops that stage and
//   reports loudly (no fallback rendering).
//
// Bindings the scaffold fixes (group 0): 0 = effect_sampler (declared
// by the ABI header), 1 = glue_u uniforms, 2 = glue_input (previous
// stage), 3 = glue_backdrop, 4 = glue_sharp. The plugin discovers
// locations by reflection after link — naga's naming is not an API.

use std::ffi::{c_char, CStr};

/// The entry scaffold appended to every wire module. Reads the previous
/// stage's color into `ctx.color`, calls the module's `apply`, returns
/// the resulting color. Position = raw gl_FragCoord (device px, GL
/// origin), matching the GlassOptics box-relative convention.
const GLUE_ENTRY: &str = r#"
@group(0) @binding(1) var<uniform> glue_u: CustomUniforms;
@group(0) @binding(2) var glue_input: texture_2d<f32>;
@group(0) @binding(3) var glue_backdrop: texture_2d<f32>;
@group(0) @binding(4) var glue_sharp: texture_2d<f32>;

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(glue_input));
    let uv = pos.xy / dims;
    var ctx = effect_ctx_identity(pos.xy);
    // Host fill (Phase 5a): mirrors ENGINE_ENTRY — slot 7 carries the
    // union-box-local card rect; `runStage` overwrites the DSL's empty
    // slot-7 vec4 before upload. Keep the two in step (D-M7).
    ctx.card_rect = glue_u.data[7];
    ctx.color = textureSample(glue_input, effect_sampler, uv);
    let out_ctx = apply(ctx, glue_backdrop, glue_sharp, glue_u);
    return out_ctx.color;
}
"#;

/// Transpile one complete wire module to GLSL ES 3.00.
pub fn transpile(complete_wgsl: &str) -> Result<String, String> {
    let full = format!("{complete_wgsl}\n{GLUE_ENTRY}");
    let module = naga::front::wgsl::parse_str(&full).map_err(|e| format!("wgsl parse: {e}"))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| format!("wgsl validate: {e}"))?;
    let options = naga::back::glsl::Options {
        version: naga::back::glsl::Version::new_gles(300),
        // Raw gl_FragCoord: the plugin's capture rows are used directly
        // as GL rows (see GlassOptics.hpp) — no Y flip anywhere.
        writer_flags: naga::back::glsl::WriterFlags::empty(),
        binding_map: naga::back::glsl::BindingMap::default(),
        zero_initialize_workgroup_memory: false,
    };
    let pipeline = naga::back::glsl::PipelineOptions {
        shader_stage: naga::ShaderStage::Fragment,
        entry_point: "fs_main".to_string(),
        multiview: None,
    };
    let mut out = String::new();
    let mut writer = naga::back::glsl::Writer::new(
        &mut out,
        &module,
        &info,
        &options,
        &pipeline,
        naga::proc::BoundsCheckPolicies::default(),
    )
    .map_err(|e| format!("glsl init: {e:?}"))?;
    writer.write().map_err(|e| format!("glsl write: {e}"))?;
    Ok(out)
}

/// ABI version of this glue (bump when the C API or scaffold changes).
pub const RIODE_SHADER_GLUE_ABI_VERSION: u32 = 1;

pub type TranspileFn = unsafe extern "C" fn(
    wgsl: *const c_char,
    glsl_buf: *mut c_char,
    glsl_cap: usize,
    err_buf: *mut c_char,
    err_cap: usize,
) -> i32;

/// Stable C surface, mirroring `riode_item_helper_v1_api`. The plugin
/// dlsyms `riode_shader_glue_v1_api` and checks `abi_version`.
#[repr(C)]
pub struct riode_shader_glue_v1_api {
    pub abi_version: u32,
    /// Returns 1 with GLSL written to `glsl_buf` (NUL-terminated, may
    /// be truncated-but-successful only if it fits), 0 with a reason in
    /// `err_buf`. Buffers must be non-null; `cap` includes the NUL.
    pub transpile: TranspileFn,
}

/// Copy `s` into a caller-owned C buffer. False when it does not fit.
///
/// # Safety
/// `buf` must be valid for `cap` bytes.
unsafe fn write_cstr(buf: *mut c_char, cap: usize, s: &str) -> bool {
    if buf.is_null() || cap == 0 {
        return false;
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min(cap - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast::<u8>(), n);
    *buf.add(n) = 0;
    n == bytes.len()
}

unsafe extern "C" fn c_transpile(
    wgsl: *const c_char,
    glsl_buf: *mut c_char,
    glsl_cap: usize,
    err_buf: *mut c_char,
    err_cap: usize,
) -> i32 {
    if wgsl.is_null() {
        write_cstr(err_buf, err_cap, "null wgsl input");
        return 0;
    }
    let Ok(src) = CStr::from_ptr(wgsl).to_str() else {
        write_cstr(err_buf, err_cap, "wgsl input is not valid UTF-8");
        return 0;
    };
    match transpile(src) {
        Ok(glsl) => {
            if write_cstr(glsl_buf, glsl_cap, &glsl) {
                1
            } else {
                write_cstr(err_buf, err_cap, "glsl buffer too small");
                0
            }
        }
        Err(e) => {
            write_cstr(err_buf, err_cap, &e);
            0
        }
    }
}

#[no_mangle]
pub static riode_shader_glue_v1_api: riode_shader_glue_v1_api = riode_shader_glue_v1_api {
    abi_version: RIODE_SHADER_GLUE_ABI_VERSION,
    transpile: c_transpile,
};
