use crate::foundation::types::{Color, Geo};
use crate::shadow_ext::{ShadowLayerExt, ShadowSpec};

#[derive(Clone, Debug)]
pub struct ShadowDef {
    pub color: Color,
    pub layers: Vec<ShadowLayerExt>,
    pub spread: f32,
    pub offset: [f32; 2],
    pub inset: bool,
    pub elevation: f32,
}

impl Default for ShadowDef {
    fn default() -> Self {
        Self {
            color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.3 },
            layers: vec![],
            spread: 0.0,
            offset: [0.0, 0.0],
            inset: false,
            elevation: 0.0,
        }
    }
}

impl From<ShadowDef> for ShadowSpec {
    fn from(def: ShadowDef) -> Self {
        ShadowSpec {
            color: def.color,
            layers: def.layers,
            spread: def.spread,
            offset: def.offset,
            inset: def.inset,
            elevation: def.elevation,
        }
    }
}
