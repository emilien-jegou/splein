//! Shadow rasterizer for software backends, implemented in shader crate.
//! Engines should delegate to this.

use crate::foundation::types::{Color, Geo};
use super::shadow_ext::{ShadowRenderer, ShadowSpec};
use crate::shadow::cpu_full::CpuShadowFull;
use crate::shadow::api::render_box_shadow;

#[derive(Clone)]
pub struct ShadowRasterizer {
    cpu: CpuShadowFull,
}

impl Default for ShadowRasterizer {
    fn default() -> Self {
        Self { cpu: CpuShadowFull }
    }
}

impl ShadowRasterizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(&mut self, color: Color, blur: f32, offset: [f32; 2], spread: f32, inset: bool, geo: &Geo) -> Vec<u8> {
        render_box_shadow(color, blur, offset, spread, inset, geo)
    }

    pub fn render_spec(&mut self, spec: &ShadowSpec, geo: &Geo) -> Vec<u8> {
        self.cpu.render(spec, geo)
    }
}
