// Single responsibility: Snapshotting layout rects before layout and springing the deltas back.

use smallvec::SmallVec;

use crate::foundation::ResolvedRect;
use crate::motion::controller::Controller;
use crate::motion::target::Target;
use crate::motion::transition::Transition;
use crate::motion::vector::MotionVector;
use crate::runtime::FrameScheduler;
use crate::tree::NodeId;

/// Child rects captured before layout, tagged with their transition container.
pub type FlipSnapshot = SmallVec<[(NodeId, NodeId, ResolvedRect); 16]>;

/// Builds the inverse transform that renders `new` at `old`, ready to spring back to identity.
///
/// Rotation and scale pivot about the node's centre, so the translation compensates for the
/// half-size difference as well as the origin shift.
fn inverse_flip(old: ResolvedRect, new: ResolvedRect) -> MotionVector {
    MotionVector {
        x: (old.x - new.x) - (new.width - old.width) * 0.5,
        y: (old.y - new.y) - (new.height - old.height) * 0.5,
        rotation: 0.0,
        scale_x: if new.width > f32::EPSILON {
            old.width / new.width
        } else {
            1.0
        },
        scale_y: if new.height > f32::EPSILON {
            old.height / new.height
        } else {
            1.0
        },
        opacity: 1.0,
    }
}

impl FrameScheduler {
    /// Captures child rects for every layout-transition container before layout runs.
    ///
    /// Skips the first layout and viewport resizes: neither is a content move worth springing.
    pub fn capture_flip(&mut self, root: NodeId, size: (u32, u32)) {
        self.flip_snapshot.clear();
        let previous = self.arena.get(root).resolved_rect;
        if previous.width <= 0.0
            || previous.width != size.0 as f32
            || previous.height != size.1 as f32
        {
            return;
        }
        for container in self.arena.layout_transitions() {
            for &child in self.arena.children(container) {
                let rect = self.arena.get(child).resolved_rect;
                self.flip_snapshot.push((container, child, rect));
            }
        }
    }

    /// Springs every captured child that moved, presenting its first frame immediately.
    pub fn apply_flip(&mut self) {
        if self.flip_snapshot.is_empty() {
            return;
        }
        for (container, child, old) in std::mem::take(&mut self.flip_snapshot) {
            if !self.arena.is_valid(child) {
                continue;
            }
            let new = self.arena.get(child).resolved_rect;
            if new == old {
                continue;
            }
            let Some(transition) = self.arena.get(container).state.layout_transition else {
                continue;
            };
            let from = inverse_flip(old, new);
            if from == MotionVector::NEUTRAL {
                continue;
            }
            // Motion ownership: never spring a node something else is already moving.
            if self.arena.get(child).state.motion != crate::motion::MotionState::NEUTRAL {
                continue;
            }
            let built = match transition {
                Transition::Timed(tween) => {
                    Controller::tween(child, from, MotionVector::NEUTRAL, tween)
                }
                Transition::Spring(spring) => {
                    Controller::spring(child, from, MotionVector::NEUTRAL, spring)
                }
            };
            let mut controller = built.targeting(Target::Motion);
            if let Some(id) = controller.advance(&mut self.arena, 0.0) {
                self.dirty_nodes_this_frame.push(id);
            }
            self.motion.push(controller);
        }
    }
}
