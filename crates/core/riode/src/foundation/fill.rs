// Visual surface fill — the one paint primitive the purge leaves.
//
// `Fill` *is* a `Stage`: a solid colour written into its own block bank
// and ORed into `stage_mask`, so `material=Fill::solid(...)` is an
// ordinary one-stage chain. There is no gradient vocabulary (linear /
// radial were deleted with the legacy rect path) and no `DrawRect`
// behind it — the stage is the paint.

use crate::foundation::color::Color;
use crate::foundation::geo::Geo;
use crate::foundation::shading::{
    K_MODULAR_FLOATS, Stage, W_FILL, W_STAGEMASK, stage_mask, MODULAR_GLASS_WGSL,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    Solid(Color),
}

impl Fill {
    pub fn solid(color: Color) -> Self {
        Self::Solid(color)
    }

    /// The colour this fill paints, for the few readers (engine
    /// passthroughs) that still want the raw value.
    #[inline(always)]
    pub fn color(&self) -> Color {
        match self {
            Self::Solid(c) => *c,
        }
    }
}

impl Stage for Fill {
    fn shader(&self) -> &'static str {
        MODULAR_GLASS_WGSL
    }

    fn write_block(&self, _geo: &Geo, block: &mut [f32; K_MODULAR_FLOATS], _dpr: f32) {
        block[W_STAGEMASK] = (block[W_STAGEMASK] as u32 | stage_mask::FILL) as f32;
        // Fill (w8): r, g, b, a — straight (unpremultiplied); the
        // shader premultiplies against coverage at the output.
        let c = self.color();
        block[W_FILL] = c.r;
        block[W_FILL + 1] = c.g;
        block[W_FILL + 2] = c.b;
        block[W_FILL + 3] = c.a;
    }

    // pad 0, sigma 0 (trait defaults): the fill paints strictly inside
    // the silhouette and needs no backdrop pre-blur.
}

impl From<Color> for Fill {
    fn from(c: Color) -> Self {
        Fill::solid(c)
    }
}

impl From<&str> for Fill {
    fn from(s: &str) -> Self {
        Fill::solid(s.into())
    }
}

impl From<String> for Fill {
    fn from(s: String) -> Self {
        Fill::solid(s.as_str().into())
    }
}

impl From<u32> for Fill {
    fn from(hex: u32) -> Self {
        Fill::solid(Color::hex(hex))
    }
}

impl From<i32> for Fill {
    fn from(hex: i32) -> Self {
        Fill::solid(Color::hex(hex as u32))
    }
}
