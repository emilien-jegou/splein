// Single responsibility: First-class Blend primitive for material stages, effect chains, and leaf elements.

use riode::foundation::{BlendMode, EffectStage, Geo, Stage};
use riode::reactive::prop::{IntoProp, Prop};
use riode::foundation::Effect;

pub const BLEND_SEPARABLE_WGSL: &str = include_str!("blend_separable.wgsl");
pub const BLEND_NONSEPARABLE_WGSL: &str = include_str!("blend_nonseparable.wgsl");
pub const BLEND_DISPATCH_WGSL: &str = include_str!("blend.wgsl");
pub const COMPOSITE_BLEND_WGSL: &str = include_str!("composite_blend.wgsl");

/// The complete WGSL color blending function library:
pub const BLEND_LIBRARY_WGSL: &str = concat!(
    include_str!("blend_separable.wgsl"),
    "\n",
    include_str!("blend_nonseparable.wgsl"),
    "\n",
    include_str!("blend.wgsl"),
);

/// First-class Blend primitive. Can be used in `effect=[...]`, `material=[...]`, or on leaf props.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Blend {
    pub mode: BlendMode,
}

#[allow(non_upper_case_globals)]
impl Blend {
    pub const fn new(mode: BlendMode) -> Self {
        Self { mode }
    }

    // Direct constructors matching Fill::new()
    pub const fn normal() -> Self { Self::new(BlendMode::Normal) }
    pub const fn multiply() -> Self { Self::new(BlendMode::Multiply) }
    pub const fn screen() -> Self { Self::new(BlendMode::Screen) }
    pub const fn overlay() -> Self { Self::new(BlendMode::Overlay) }
    pub const fn additive() -> Self { Self::new(BlendMode::Additive) }
    pub const fn darken() -> Self { Self::new(BlendMode::Darken) }
    pub const fn lighten() -> Self { Self::new(BlendMode::Lighten) }
    pub const fn color_dodge() -> Self { Self::new(BlendMode::ColorDodge) }
    pub const fn color_burn() -> Self { Self::new(BlendMode::ColorBurn) }
    pub const fn hard_light() -> Self { Self::new(BlendMode::HardLight) }
    pub const fn soft_light() -> Self { Self::new(BlendMode::SoftLight) }
    pub const fn difference() -> Self { Self::new(BlendMode::Difference) }
    pub const fn exclusion() -> Self { Self::new(BlendMode::Exclusion) }
    pub const fn hue() -> Self { Self::new(BlendMode::Hue) }
    pub const fn saturation() -> Self { Self::new(BlendMode::Saturation) }
    pub const fn color() -> Self { Self::new(BlendMode::Color) }
    pub const fn luminosity() -> Self { Self::new(BlendMode::Luminosity) }

    // Associated constants so <Group effect={Blend::Multiply}> works directly
    pub const Normal: Self = Self::normal();
    pub const Multiply: Self = Self::multiply();
    pub const Screen: Self = Self::screen();
    pub const Overlay: Self = Self::overlay();
    pub const Additive: Self = Self::additive();
    pub const Darken: Self = Self::darken();
    pub const Lighten: Self = Self::lighten();
    pub const ColorDodge: Self = Self::color_dodge();
    pub const ColorBurn: Self = Self::color_burn();
    pub const HardLight: Self = Self::hard_light();
    pub const SoftLight: Self = Self::soft_light();
    pub const Difference: Self = Self::difference();
    pub const Exclusion: Self = Self::exclusion();
    pub const Hue: Self = Self::hue();
    pub const Saturation: Self = Self::saturation();
    pub const Color: Self = Self::color();
    pub const Luminosity: Self = Self::luminosity();
}

impl From<BlendMode> for Blend {
    fn from(mode: BlendMode) -> Self {
        Self { mode }
    }
}

impl From<Blend> for BlendMode {
    fn from(b: Blend) -> Self {
        b.mode
    }
}

// 1. Blend as an EffectStage in subtree effect chains:
// <Group effect={Blend::Multiply}> or <Group effect=[Effect::wave(..), Blend::Screen]>
impl EffectStage for Blend {
    fn wgsl_source(&self) -> &'static str {
        ""
    }
    fn uniform_size(&self) -> usize {
        0
    }
    fn write_uniforms(&self, _geo: &Geo, _buffer: &mut [u8]) {}
    fn blend_mode(&self) -> Option<BlendMode> {
        Some(self.mode)
    }
    fn requires_backdrop(&self) -> bool {
        !self.mode.is_normal()
    }
}

// 2. Blend as a Material Stage in surface/overlay chains:
// material=[Fill::solid(...), Blend::color_dodge()]
impl Stage for Blend {
    fn wgsl_source(&self) -> &'static str {
        ""
    }
    fn uniform_size(&self) -> usize {
        0
    }
    fn write_uniforms(&self, _geo: &Geo, _buffer: &mut [u8]) {}
    fn blend_mode(&self) -> BlendMode {
        self.mode
    }
    fn requires_backdrop(&self) -> bool {
        !self.mode.is_normal()
    }
}

impl IntoProp<Effect> for Blend {
    fn into_prop(self) -> Prop<Effect> {
        Prop::Static(Effect::from(self.mode))
    }
}
