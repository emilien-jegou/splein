use crate::foundation::types::{Color, Geo};
use crate::shadow_ext::{ShadowLayerExt, ShadowSpec};

pub fn footprint(spec: &ShadowSpec) -> f32 {
    let mut m = 0.0f32;
    for l in &spec.layers {
        let v = l.extent + l.softness * 3.0;
        if v > m { m = v; }
    }
    m + spec.spread + spec.offset[0].abs().max(spec.offset[1].abs())
}

pub fn alpha_for(spec: &ShadowSpec, dist: f32) -> f32 {
    let mut a = 0.0f32;
    for l in &spec.layers {
        if dist <= l.extent + l.softness * 3.0 {
            let t = ((l.extent - dist) / l.softness.max(1e-6)).max(0.0).min(1.0);
            a += l.opacity * t;
        }
    }
    a * spec.color.a
}
