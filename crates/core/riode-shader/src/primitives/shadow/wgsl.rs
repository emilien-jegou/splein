pub const SHADOW_WGSL_COMMON: &str = r#"
struct ShadowParams {
  color: vec4<f32>,
  offset: vec2<f32>,
  blur: f32,
  spread: f32,
  inset: f32,
};
"#;
