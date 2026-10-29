use crate::foundation::types::{Color, Geo};
use crate::shadow_ext::{ShadowRenderer, ShadowSpec};

#[derive(Clone, Default)]
pub struct CpuShadowRendererImpl;

impl ShadowRenderer for CpuShadowRendererImpl {
    fn render(&mut self, _spec: &ShadowSpec, _geo: &Geo) -> Vec<u8> {
        vec![]
    }
}

pub fn render_shadow(spec: &ShadowSpec, geo: &Geo) -> Vec<u8> {
    CpuShadowRendererImpl::default().render(spec, geo)
}
