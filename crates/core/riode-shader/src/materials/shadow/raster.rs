use crate::foundation::types::Geo;
use crate::shadow_ext::{ShadowRenderer, ShadowSpec};

pub fn compute_pad(spec: &ShadowSpec) -> f32 {
    let mut pad = 0.0f32;
    for layer in &spec.layers {
        let p = layer.extent.max(3.0 * layer.softness.max(0.5));
        if p > pad { pad = p; }
    }
    pad += spec.spread;
    if pad < 0.0 { pad = 0.0; }
    pad
}

pub fn prepare(_spec: &ShadowSpec, _geo: &Geo) -> Vec<u8> {
    vec![]
}
