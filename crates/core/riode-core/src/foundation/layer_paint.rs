// Single responsibility: The bundle of optional stage chains a node carries.
//
// `shape`, `material`, `overlay`, and `effect` always travel together — from
// reconciliation through compile, the scene stream, and into the engine — and
// every hop used to re-list them as four parallel `Option` fields, cloning
// each one separately. They are one value with one identity, so they cross
// every boundary as one field.

use crate::foundation::{Effect, Material, Overlay, ShapeContainer};
use std::fmt;

/// What a node paints: the four optional stage chains, as one value.
///
/// Equality is identity of the shared chains (each field compares by `Arc`
/// pointer, see the field types), so a `LayerPaint` compares and clones as
/// cheaply as its parts did — it just can no longer be partially copied or
/// dropped at a pipeline boundary.
#[derive(Clone, Default, PartialEq)]
pub struct LayerPaint {
    pub shape: Option<ShapeContainer>,
    pub material: Option<Material>,
    pub overlay: Option<Overlay>,
    pub effect: Option<Effect>,
}

impl LayerPaint {
    /// Whether any chain is present. A present chain opens the node's
    /// offscreen layer, so this is the layer decision itself.
    #[inline(always)]
    pub fn opens_layer(&self) -> bool {
        self.shape.is_some()
            || self.material.is_some()
            || self.overlay.is_some()
            || self.effect.is_some()
    }

    /// Identity of all four chains, for cross-frame cache keys.
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.shape == other.shape
            && self.material == other.material
            && self.overlay == other.overlay
            && self.effect == other.effect
    }
}

impl fmt::Debug for LayerPaint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LayerPaint")
            .field("shape", &self.shape)
            .field("material", &self.material)
            .field("overlay", &self.overlay)
            .field("effect", &self.effect)
            .finish()
    }
}
