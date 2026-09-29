// Single responsibility: Abstract rendering contract for display list rasterization backends.

use crate::foundation::DamageRegion;
use crate::scene::Scene;

/// Backend-agnostic rasterization contract executing display list commands.
pub trait Renderer {
    /// Error type produced during rasterization or presentation.
    type Error;

    /// Renders a compiled display list scene with damaged regions.
    fn render(&mut self, scene: &Scene, damage: &DamageRegion) -> Result<(), Self::Error>;
}
