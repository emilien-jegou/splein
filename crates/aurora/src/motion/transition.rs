// Single responsibility: Declarative transitions steering bound property changes into controllers.

use crate::motion::controller::Controller;
use crate::motion::spring::Spring;
use crate::motion::state::MotionState;
use crate::motion::target::Target;
use crate::motion::tween::Tween;
use crate::motion::vector::MotionVector;
use crate::runtime::FrameScheduler;
use crate::tree::NodeId;

/// Interpolation declared on a builder and applied when a bound property changes.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Transition {
    /// Fixed-duration interpolation under an easing curve.
    Timed(Tween),
    /// Physics-based settling.
    Spring(Spring),
}

impl FrameScheduler {
    /// Steers changed bound values into controllers, restoring their starting values.
    /// Returns whether the change was redirected instead of being committed directly.
    pub fn redirect_change(
        &mut self,
        node: NodeId,
        previous: MotionVector,
        transform_changed: bool,
        opacity_changed: bool,
    ) -> bool {
        if !transform_changed && !opacity_changed {
            return false;
        }
        let Some(transition) = self.arena.get(node).state.transition else {
            return false;
        };
        let current = {
            let n = self.arena.get(node);
            MotionVector::from_state(&MotionState {
                transform: n.transform,
                opacity: n.style.appearance.opacity,
            })
        };

        // Restore the starting values so this frame presents the animation's first frame.
        if transform_changed {
            self.arena.get_mut(node).transform = previous.to_state().transform;
        }
        if opacity_changed {
            self.arena.get_mut(node).style.appearance.opacity = previous.opacity;
        }

        let steered = [
            (transform_changed, Target::Transform),
            (opacity_changed, Target::Opacity),
        ];
        for (changed, target) in steered {
            if !changed {
                continue;
            }
            let controller = match transition {
                Transition::Timed(tween) => Controller::tween(node, previous, current, tween),
                Transition::Spring(spring) => Controller::spring(node, previous, current, spring),
            };
            self.motion.push(controller.targeting(target));
        }
        true
    }
}
