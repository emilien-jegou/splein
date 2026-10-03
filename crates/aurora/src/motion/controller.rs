// Single responsibility: One in-flight animation writing one slot of a retained node.

use crate::foundation::Key;
use crate::motion::mode::Mode;
use crate::motion::playback::Playback;
use crate::motion::spring::Spring;
use crate::motion::target::Target;
use crate::motion::tween::Tween;
use crate::motion::vector::MotionVector;
use crate::tree::{DirtyFlags, NodeId, TreeArena};

/// One in-flight animation writing one slot of a retained view.
pub struct Controller {
    node: NodeId,
    key: Option<Key>,
    target: Target,
    from: MotionVector,
    to: MotionVector,
    mode: Mode,
    playback: Playback,
    delay: f32,
}

impl Controller {
    /// Creates a tween-driven controller writing the motion override of `node`.
    pub fn tween(node: NodeId, from: MotionVector, to: MotionVector, tween: Tween) -> Self {
        Self::new(node, from, to, Mode::tween(tween))
    }

    /// Creates a spring-driven controller writing the motion override of `node`.
    pub fn spring(node: NodeId, from: MotionVector, to: MotionVector, spring: Spring) -> Self {
        Self::new(node, from, to, Mode::spring(spring, from, to))
    }

    fn new(node: NodeId, from: MotionVector, to: MotionVector, mode: Mode) -> Self {
        Self {
            node,
            key: None,
            target: Target::Motion,
            from,
            to,
            mode,
            playback: Playback::Playing,
            delay: 0.0,
        }
    }

    /// Associates the controller with the key that created it, enabling key-based retargeting.
    pub fn with_key(mut self, key: Key) -> Self {
        self.key = Some(key);
        self
    }

    /// Redirects the controller to a declarative slot instead of the motion override.
    pub fn targeting(mut self, target: Target) -> Self {
        self.target = target;
        self
    }

    /// Holds the controller idle for `seconds` before its first step.
    pub fn with_delay(mut self, seconds: f32) -> Self {
        self.delay = seconds.max(0.0);
        self
    }

    /// Retained node this controller writes to.
    #[inline(always)]
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Key that created this controller, when it was created imperatively.
    #[inline(always)]
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }

    /// Slot this controller writes to.
    #[inline(always)]
    pub fn target(&self) -> Target {
        self.target
    }

    /// Current playback lifecycle.
    #[inline(always)]
    pub fn playback(&self) -> Playback {
        self.playback
    }

    /// Resumes playback from a paused or idle state.
    pub fn resume(&mut self) {
        if matches!(self.playback, Playback::Paused | Playback::Idle) {
            self.playback = Playback::Playing;
        }
    }

    /// Suspends playback, retaining all progress.
    pub fn pause(&mut self) {
        if self.playback == Playback::Playing {
            self.playback = Playback::Paused;
        }
    }

    /// Abandons the animation, leaving the view where it stands.
    pub fn cancel(&mut self) {
        self.playback = Playback::Finished;
    }

    /// Current component velocities, which a retarget carries over.
    #[inline(always)]
    pub fn velocity(&self) -> MotionVector {
        self.mode.velocity()
    }

    /// Adopts a launch velocity before the first step of a retargeted spring.
    #[inline(always)]
    pub fn adopt_velocity(&mut self, velocity: MotionVector) {
        self.mode.adopt_velocity(velocity);
    }

    /// Whether the controller has settled, been cancelled, or lost its view.
    #[inline(always)]
    pub fn is_finished(&self) -> bool {
        self.playback == Playback::Finished
    }

    /// Advances by `dt` seconds, writing the result and returning the node touched.
    pub fn advance(&mut self, arena: &mut TreeArena, dt: f32) -> Option<NodeId> {
        if !arena.is_valid(self.node) || self.playback == Playback::Finished {
            self.playback = Playback::Finished;
            return None;
        }
        if self.delay > 0.0 {
            if self.delay >= dt {
                self.delay -= dt;
                return None;
            }
            self.delay = 0.0;
        }

        let (value, settled) = if self.playback == Playback::Playing {
            self.mode.step(self.from, self.to, dt)
        } else {
            (self.mode.value(self.from, self.to), false)
        };
        if settled {
            self.playback = Playback::Finished;
        }

        if !self.target.write(arena.get_mut(self.node), value) {
            return None;
        }
        arena.mark_dirty(self.node, DirtyFlags::PAINT);
        Some(self.node)
    }
}
