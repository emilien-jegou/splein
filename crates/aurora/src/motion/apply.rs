// Single responsibility: Applying keyed motion overrides to retained nodes with paint invalidation.

use crate::foundation::Key;
use crate::motion::MotionState;
use crate::runtime::FrameScheduler;
use crate::tree::DirtyFlags;

/// Applies a motion override to a keyed node, invalidating paint only when it changed.
pub fn apply_motion(
    scheduler: &mut FrameScheduler,
    key: &Key,
    update: impl FnOnce(&mut MotionState),
) -> bool {
    let Some(id) = scheduler.arena.node_for_key(key) else {
        return false;
    };
    let changed = {
        let node = scheduler.arena.get_mut(id);
        let before = node.state.motion;
        update(&mut node.state.motion);
        node.state.motion != before
    };
    if changed {
        scheduler.arena.mark_dirty(id, DirtyFlags::PAINT);
        scheduler.pending_updates.push((id, DirtyFlags::PAINT));
    }
    changed
}
