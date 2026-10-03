// Single responsibility: Selecting and writing the node slot a controller animates.

use crate::motion::vector::MotionVector;
use crate::tree::LayoutNode;

/// Slot a controller writes its interpolated value into.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// Compositor override layered above declarative style.
    Motion,
    /// Declarative transform owned by a bound property.
    Transform,
    /// Declarative opacity owned by a bound property.
    Opacity,
}

impl Target {
    /// Writes the interpolated value into its slot, reporting whether it changed.
    pub(crate) fn write(self, node: &mut LayoutNode, value: MotionVector) -> bool {
        match self {
            Self::Motion => {
                let state = value.to_state();
                if node.state.motion == state {
                    return false;
                }
                node.state.motion = state;
            }
            Self::Transform => {
                let transform = value.to_state().transform;
                if node.transform == transform {
                    return false;
                }
                node.transform = transform;
            }
            Self::Opacity => {
                let opacity = value.opacity.clamp(0.0, 1.0);
                if node.style.appearance.opacity == opacity {
                    return false;
                }
                node.style.appearance.opacity = opacity;
            }
        }
        true
    }
}
