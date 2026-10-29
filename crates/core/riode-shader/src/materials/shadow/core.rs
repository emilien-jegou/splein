use crate::foundation::types::{Color, Geo};
use crate::shadow_ext::{ShadowLayerExt, ShadowSpec};

pub fn make_spec(color: Color, elevation: f32, blur: f32, offset: [f32; 2], spread: f32, inset: bool) -> ShadowSpec {
    let layers = if elevation > 0.0 || blur > 0.0 {
        vec![ShadowLayerExt { softness: (blur.max(elevation*0.75))*0.5, extent: blur.max(elevation*2.0), opacity: 0.25 }]
    } else { vec![] };
    ShadowSpec { color, layers, spread, offset, inset, elevation }
}

pub fn layer_pad(spec: &ShadowSpec) -> f32 {
    let mut pad = 0.0f32;
    for l in &spec.layers {
        let p = l.extent.max(3.0*l.softness.max(0.5));
        if p > pad { pad = p; }
    }
    pad + spec.spread
}
