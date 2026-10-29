pub use riode_shader::materials::blend::{
    BLEND_COMPOSITE_COMBINED_WGSL,
    BLEND_NONSEPARABLE_WGSL,
    BLEND_SEPARABLE_WGSL,
    BLEND_WGSL,
    COMPOSITE_BLEND_WGSL,
};
pub use riode_shader::materials::blur::BLUR_WGSL;
pub use riode_shader::materials::composite::COMPOSITE_WGSL;
pub use riode_shader::materials::glass::MODULAR_GLASS_WGSL;
// Shadow shader moved; keep compatibility if possible
pub const SHADOW_WGSL: &str = "";
pub use riode_shader::materials::{EFFECT_ABI_WGSL, ENGINE_ENTRY_WGSL};
