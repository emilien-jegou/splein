use crate::foundation::types::{Color, Geo};
use crate::shadow_ext::{ShadowLayerExt, ShadowSpec};

pub fn layer_to_ext(softness: f32, extent: f32, opacity: f32) -> ShadowLayerExt {
    ShadowLayerExt { softness, extent, opacity }
}

pub fn shadow_spec(color: Color, layers: Vec<ShadowLayerExt>, spread: f32, offset: [f32; 2], inset: bool, elevation: f32) -> ShadowSpec {
    ShadowSpec { color, layers, spread, offset, inset, elevation }
}
