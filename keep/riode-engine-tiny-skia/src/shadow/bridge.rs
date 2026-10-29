use riode_shader::shadow_ext::{CpuShadowRenderer, ShadowRenderer, ShadowSpec};
use riode_shader::foundation::types::Geo;

#[derive(Clone, Default)]
pub struct UnifiedShadow {
    pub renderer: CpuShadowRenderer,
}

impl UnifiedShadow {
    pub fn new() -> Self { Self::default() }
    pub fn render(&mut self, spec: &ShadowSpec, geo: &Geo) -> Vec<u8> {
        self.renderer.render(spec, geo)
    }
}
