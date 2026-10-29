use crate::foundation::types::{Color, Geo};
use crate::shadow_ext::{ShadowLayerExt, ShadowSpec, ShadowRenderer};
use crate::shadow::cpu_full::CpuShadowFull;

pub fn render_box_shadow(color: Color, blur: f32, offset: [f32; 2], spread: f32, inset: bool, geo: &Geo) -> Vec<u8> {
    let mut layers = Vec::new();
    if blur > 0.0 {
        layers.push(ShadowLayerExt { softness: blur*0.5, extent: blur, opacity: 1.0 });
    }
    let spec = ShadowSpec { color, layers, spread, offset, inset, elevation: blur.max(0.0) };
    CpuShadowFull.render(&spec, geo)
}
