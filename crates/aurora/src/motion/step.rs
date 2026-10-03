// Single responsibility: Advancing motion controllers into retained nodes each frame.

use crate::runtime::FrameScheduler;
use crate::tree::DirtyFlags;

impl FrameScheduler {
    /// Advances every running controller and published spring by one frame delta.
    pub fn step_motion(&mut self) {
        let dt = self.motion.tick();
        if self.motion.is_idle() {
            return;
        }
        self.motion.step_publishers(dt);
        let Self {
            motion,
            arena,
            pending_updates,
            ..
        } = self;
        for controller in motion.controllers_mut() {
            if let Some(id) = controller.advance(arena, dt) {
                pending_updates.push((id, DirtyFlags::PAINT));
            }
        }
        motion.retain_running();
    }
}
