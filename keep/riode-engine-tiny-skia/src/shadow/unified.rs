use riode_shader::shadow_ext::{CpuShadowRenderer, ShadowRenderer, ShadowSpec};
use riode_shader::foundation::types::Geo;

pub struct UnifiedShadow {
    renderer: CpuShadowRenderer,
}

impl UnifiedShadow {
    pub fn new() -> Self {
        Self { renderer: CpuShadowRenderer }
    }
    pub fn render(&mut self, spec: &ShadowSpec, geo: &Geo) -> Vec<u8> {
        self.renderer.render(spec, geo)
    }
}

impl Default for UnifiedShadow {
    fn default() -> Self {
        Self::new()
    }
}
