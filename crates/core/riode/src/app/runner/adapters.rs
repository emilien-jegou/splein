// Single responsibility: Adapters binding engine damage and app extensions to the backend contract.

use crate::app::extension::AppExtension;
use crate::app::runner::backend::{DamageResolver, FramePostProcess};
use crate::foundation::DamageRegion;
use crate::runtime::Engine;

/// Resolves swapchain-age damage from the engine's frame history.
pub struct EngineDamage<'e> {
    /// Engine whose damage ring backs the age lookup.
    engine: &'e Engine,
}

impl<'e> EngineDamage<'e> {
    /// Wraps an engine for damage-by-age resolution.
    #[inline(always)]
    pub fn new(engine: &'e Engine) -> Self {
        Self { engine }
    }
}

impl DamageResolver for EngineDamage<'_> {
    #[inline]
    fn for_age(&mut self, age: u8) -> DamageRegion {
        self.engine.damage_for_age(age)
    }
}

/// Runs the scanline extension hook across every registered application extension.
pub struct ExtensionScanline<'e> {
    /// Registered application extensions.
    extensions: &'e mut [Box<dyn AppExtension>],
}

impl<'e> ExtensionScanline<'e> {
    /// Wraps the extension list for framebuffer post-processing.
    #[inline(always)]
    pub fn new(extensions: &'e mut [Box<dyn AppExtension>]) -> Self {
        Self { extensions }
    }
}

impl FramePostProcess for ExtensionScanline<'_> {
    fn apply(&mut self, buffer: &mut [u32], damage: &DamageRegion, stride: usize, height: usize) {
        for ext in self.extensions.iter_mut() {
            ext.on_present(buffer, damage, stride, height);
        }
    }
}
