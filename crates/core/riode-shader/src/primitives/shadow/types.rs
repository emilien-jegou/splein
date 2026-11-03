use riode::foundation::BlendMode;

use crate::foundation::{Color, FalloffCurve, LightEnvironment};
use super::shadow_ext::{ShadowLayerExt, ShadowSpec};

/// Which silhouette the shadow is cast from.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ShadowShape {
    #[default]
    Box,
    Alpha,
}

/// Where the shadow offset comes from.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ShadowOffset {
    Elevation,
    Explicit { offset_x: f32, offset_y: f32 },
}

/// One composited falloff layer inside a [`Shadow`].
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ShadowLayer {
    pub falloff: FalloffCurve,
    pub softness: f32,
    pub extent: f32,
    pub opacity: f32,
    pub offset: Option<(f32, f32)>,
}

/// Complete shadow specification: one shadow per node, internally layered.
#[derive(Clone, Debug, PartialEq)]
pub struct Shadow {
    pub elevation: f32,
    pub color: Color,
    pub blend: BlendMode,
    pub shape: ShadowShape,
    pub offset: ShadowOffset,
    pub spread: f32,
    pub inset: bool,
    layers: Vec<ShadowLayer>,
}

impl Shadow {
    pub fn elevation(elevation: f32) -> Self {
        Self {
            elevation,
            color: Color::BLACK,
            blend: BlendMode::Multiply,
            shape: ShadowShape::Box,
            offset: ShadowOffset::Elevation,
            spread: 0.0,
            inset: false,
            layers: vec![ShadowLayer {
                falloff: FalloffCurve::EaseOut,
                softness: elevation * 0.75,
                extent: elevation * 2.0,
                opacity: 0.25,
                offset: None,
            }],
        }
    }

    pub fn web_like(offset_x: f32, offset_y: f32, blur: f32, spread: f32, color: Color) -> Self {
        Self {
            elevation: 0.0,
            color,
            blend: BlendMode::Normal,
            shape: ShadowShape::Box,
            offset: ShadowOffset::Explicit { offset_x, offset_y },
            spread,
            inset: false,
            layers: vec![ShadowLayer {
                falloff: FalloffCurve::Linear,
                softness: (blur * 0.5).max(0.0),
                extent: blur.max(0.0),
                opacity: 1.0,
                offset: None,
            }],
        }
    }

    pub fn layered(color: Color) -> Self {
        Self {
            elevation: 0.0,
            color,
            blend: BlendMode::Normal,
            shape: ShadowShape::Box,
            offset: ShadowOffset::Explicit { offset_x: 0.0, offset_y: 0.0 },
            spread: 0.0,
            inset: false,
            layers: Vec::new(),
        }
    }

    pub fn drop(mut self, offset_x: f32, offset_y: f32, blur: f32) -> Self {
        self.layers.push(ShadowLayer {
            falloff: FalloffCurve::Linear,
            softness: (blur * 0.5).max(0.0),
            extent: blur.max(0.0),
            opacity: 1.0,
            offset: Some((offset_x, offset_y)),
        });
        self
    }

    pub fn drops(mut self, layers: &[(f32, f32, f32)]) -> Self {
        for &(ox, oy, blur) in layers {
            self = self.drop(ox, oy, blur);
        }
        self
    }

    pub fn ease_out(mut self, softness: f32, extent: f32, opacity: f32) -> Self {
        self.layers.push(ShadowLayer {
            falloff: FalloffCurve::EaseOut,
            softness,
            extent,
            opacity,
            offset: None,
        });
        self
    }

    pub fn physical(mut self, softness: f32, extent: f32, opacity: f32) -> Self {
        self.layers.push(ShadowLayer {
            falloff: FalloffCurve::Physical,
            softness,
            extent,
            opacity,
            offset: None,
        });
        self
    }

    pub fn multiply(mut self) -> Self {
        self.blend = BlendMode::Multiply;
        self
    }

    pub fn multiply_if(mut self, on: bool) -> Self {
        self.blend = if on { BlendMode::Multiply } else { BlendMode::Normal };
        self
    }

    pub fn normal(mut self) -> Self {
        self.blend = BlendMode::Normal;
        self
    }

    pub fn blend(mut self, mode: BlendMode) -> Self {
        self.blend = mode;
        self
    }

    pub fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    pub fn falloff(mut self, falloff: FalloffCurve) -> Self {
        for layer in &mut self.layers {
            layer.falloff = falloff;
        }
        self
    }

    pub fn alpha(mut self) -> Self {
        self.shape = ShadowShape::Alpha;
        self
    }

    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }

    pub fn inset_if(mut self, on: bool) -> Self {
        self.inset = on;
        self
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = c;
        self
    }

    pub fn layers(&self) -> &[ShadowLayer] {
        &self.layers
    }

    pub fn resolve_offset(&self, light: &LightEnvironment) -> (f32, f32) {
        match self.offset {
            ShadowOffset::Elevation => light.offset_for(self.elevation),
            ShadowOffset::Explicit { offset_x, offset_y } => (offset_x, offset_y),
        }
    }

    pub fn layer_offset(&self, light: &LightEnvironment, layer: &ShadowLayer) -> (f32, f32) {
        layer.offset.unwrap_or_else(|| self.resolve_offset(light))
    }

    pub fn layer_pad(&self, layer: &ShadowLayer) -> f32 {
        self.spread + layer.extent.max(3.0 * layer.softness.max(0.5))
    }

    pub fn max_extent(&self) -> f32 {
        self.layers.iter().map(|l| l.extent.max(l.softness)).fold(0.0, f32::max)
    }

    pub fn max_softness(&self) -> f32 {
        self.layers.iter().map(|l| l.softness).fold(0.0, f32::max)
    }
}

impl From<Shadow> for ShadowSpec {
    fn from(s: Shadow) -> Self {
        ShadowSpec {
            color: s.color,
            layers: s.layers.into_iter().map(|l| ShadowLayerExt { softness: l.softness, extent: l.extent, opacity: l.opacity }).collect(),
            spread: s.spread,
            offset: match s.offset {
                ShadowOffset::Elevation => [0.0, 0.0],
                ShadowOffset::Explicit { offset_x, offset_y } => [offset_x, offset_y],
            },
            inset: s.inset,
            elevation: s.elevation,
        }
    }
}

impl Default for Shadow {
    fn default() -> Self {
        Self { color: Color::BLACK, layers: vec![], spread: 0.0, offset: ShadowOffset::Explicit { offset_x: 0.0, offset_y: 0.0 }, inset: false, elevation: 0.0, blend: BlendMode::Normal, shape: ShadowShape::Box }
    }
}

use std::str::FromStr;
impl FromStr for Shadow {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() { return Err("empty".into()); }
        let lower = s.to_lowercase();
        if lower == "none" {
            return Ok(Shadow::elevation(0.0));
        }
        let mut inset = false;
        let mut rest = lower.as_str();
        if rest.starts_with("inset ") {
            inset = true;
            rest = rest["inset ".len()..].trim();
        }
        if let Ok(e) = rest.parse::<f32>() {
            if e == 0.0 { return Ok(Shadow::elevation(0.0)); }
            let mut sh = Shadow::elevation(e);
            if inset { sh = sh.inset(); }
            return Ok(sh);
        }
        let parts: Vec<String> = rest
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|x| !x.is_empty())
            .map(|x| x.to_string())
            .collect();
        if parts.is_empty() { return Err("bad".into()); }
        let sh = match parts.as_slice() {
            [ox, oy, blur] => Shadow::web_like(ox.parse::<f32>().unwrap_or(0.0), oy.parse::<f32>().unwrap_or(0.0), blur.parse::<f32>().unwrap_or(0.0), 0.0, Color::BLACK),
            [ox, oy, blur, spread] => {
                if let Ok(sp) = spread.parse::<f32>() {
                    Shadow::web_like(ox.parse::<f32>().unwrap_or(0.0), oy.parse::<f32>().unwrap_or(0.0), blur.parse::<f32>().unwrap_or(0.0), sp, Color::BLACK)
                } else {
                    let c: Color = spread.as_str().into();
                    Shadow::web_like(ox.parse::<f32>().unwrap_or(0.0), oy.parse::<f32>().unwrap_or(0.0), blur.parse::<f32>().unwrap_or(0.0), 0.0, c)
                }
            }
            [ox, oy, blur, spread, color] => {
                if let Ok(sp) = spread.parse::<f32>() {
                    let c: Color = color.as_str().into();
                    Shadow::web_like(ox.parse::<f32>().unwrap_or(0.0), oy.parse::<f32>().unwrap_or(0.0), blur.parse::<f32>().unwrap_or(0.0), sp, c)
                } else { return Err("bad".into()); }
            }
            _ => return Err("bad".into()),
        };
        if inset { Ok(sh.inset()) } else { Ok(sh) }
    }
}

impl From<&str> for Shadow {
    fn from(s: &str) -> Self {
        Self::from_str(s).unwrap_or(Shadow::elevation(0.0))
    }
}

impl From<String> for Shadow {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
