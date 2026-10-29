use crate::foundation::types::{Color, Geo};

#[derive(Clone, Debug)]
pub struct ShadowLayerExt {
    pub softness: f32,
    pub extent: f32,
    pub opacity: f32,
}

#[derive(Clone, Debug)]
pub struct ShadowSpec {
    pub color: Color,
    pub layers: Vec<ShadowLayerExt>,
    pub spread: f32,
    pub offset: [f32; 2],
    pub inset: bool,
    pub elevation: f32,
}

pub trait ShadowRenderer {
    fn render(&mut self, spec: &ShadowSpec, geo: &Geo) -> Vec<u8>;
}

#[derive(Clone, Default)]
pub struct CpuShadowRenderer;
impl ShadowRenderer for CpuShadowRenderer {
    fn render(&mut self, _spec: &ShadowSpec, _geo: &Geo) -> Vec<u8> {
        vec![]
    }
}

#[derive(Clone, Default)]
pub struct GpuShadowRenderer;
impl ShadowRenderer for GpuShadowRenderer {
    fn render(&mut self, _spec: &ShadowSpec, _geo: &Geo) -> Vec<u8> {
        vec![]
    }
}
