// Single responsibility: Non-linear shadow falloff curves with precomputed LUTs.

/// How shadow intensity decreases with distance from the occluder.
///
/// Replaces the CSS "stack N shadows" hack with a single curve: the renderer
/// blurs once, then remaps the blurred coverage through the curve's LUT.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum FalloffCurve {
    /// Uniform coverage (legacy box-blur look).
    Linear,
    /// Crisp contact, dissolves outward (the natural default).
    EaseOut,
    /// Soft contact, tighter far edge.
    EaseIn,
    /// Smoothstep both ends.
    EaseInOut,
    /// Inverse-square-inspired approximation.
    Physical,
    /// Caller-supplied 0..1 -> 0..1 samples, nearest-neighbor.
    Custom(&'static [f32]),
}

impl FalloffCurve {
    /// Discriminant for 9-patch cache keys.
    pub const fn tag(&self) -> u8 {
        match self {
            Self::Linear => 0,
            Self::EaseOut => 1,
            Self::EaseIn => 2,
            Self::EaseInOut => 3,
            Self::Physical => 4,
            Self::Custom(_) => 5,
        }
    }

    /// Shapes normalized coverage 0..1 into 0..1.
    pub fn shape(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Self::EaseIn => t * t,
            Self::EaseInOut => t * t * (3.0 - 2.0 * t),
            Self::Physical => t.powf(1.4),
            Self::Custom(samples) => {
                if samples.is_empty() {
                    return t;
                }
                let idx =
                    ((t * (samples.len() - 1) as f32).round() as usize).min(samples.len() - 1);
                samples[idx].clamp(0.0, 1.0)
            }
        }
    }

    /// 256-entry coverage remap table for the rasterizer hot path.
    pub fn lut(&self) -> [u8; 256] {
        let mut out = [0u8; 256];
        for (i, v) in out.iter_mut().enumerate() {
            *v = (self.shape(i as f32 / 255.0) * 255.0).round() as u8;
        }
        out
    }
}

impl Default for FalloffCurve {
    fn default() -> Self {
        Self::EaseOut
    }
}
