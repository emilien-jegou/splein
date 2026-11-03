use crate::foundation::types::{Color, Geo};
use crate::shadow::blur::box_blur1d;
use super::shadow_ext::{ShadowRenderer, ShadowSpec};

#[derive(Clone, Default)]
pub struct CpuShadowFull;
impl ShadowRenderer for CpuShadowFull {
    fn render(&mut self, spec: &ShadowSpec, geo: &Geo) -> Vec<u8> {
        let w = geo.width() as usize;
        let h = geo.height() as usize;
        if w == 0 || h == 0 { return vec![]; }
        let mut mask = vec![255u8; w*h];
        let sigma = spec.elevation.max(1.0);
        let r = ((sigma * 0.5) as usize).max(1);
        let mut buf = vec![0u8; w*h];
        box_blur1d(&mask, &mut buf, w, h, r);
        // simple composite to RGBA
        let mut out = vec![0u8; w*h*4];
        for i in 0..(w*h) {
            let a = buf[i] as f32 / 255.0;
            out[i*4+0] = (spec.color.r * 255.0) as u8;
            out[i*4+1] = (spec.color.g * 255.0) as u8;
            out[i*4+2] = (spec.color.b * 255.0) as u8;
            out[i*4+3] = (a * spec.color.a * 255.0) as u8;
        }
        out
    }
}
