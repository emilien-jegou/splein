pub mod blend;
pub mod blur;
pub mod composite;
pub mod glass;
pub mod shadow;

pub use blend::*;
pub use blur::*;
pub use composite::*;
pub use glass::*;
pub use shadow::*;

/// Shared ABI for effect stages.
pub const EFFECT_ABI_WGSL: &str = r#"// Shared Effect ABI placeholder
"#;
/// Engine entry scaffold.
pub const ENGINE_ENTRY_WGSL: &str = r#"// Engine entry scaffold placeholder
@vertex
fn vs_main() -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 1.0)
    );
    return vec4<f32>(pos[vertex_index], 0.0, 1.0);
}
"#;
