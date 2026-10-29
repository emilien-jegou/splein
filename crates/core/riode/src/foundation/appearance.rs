// Aggregate visual surface appearance specification.
// Purged: all legacy visual properties removed; only material remains.

use crate::foundation::Material;

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    /// The element's material — the ordered shading chain over the
    /// sampled background.
    pub material: Option<Material>,
}

impl Appearance {
    pub const EMPTY: Self = Self { material: None };

    pub fn with_material(mut self, material: Material) -> Self {
        self.material = Some(material);
        self
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self::EMPTY
    }
}
