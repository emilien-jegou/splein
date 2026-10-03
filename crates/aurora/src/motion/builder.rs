// Single responsibility: Fluent construction of a keyed animation and its target values.

use crate::foundation::Key;
use crate::motion::controller::Controller;
use crate::motion::spring::Spring;
use crate::motion::tween::Tween;
use crate::motion::vector::MotionVector;
use crate::motion::Ease;
use crate::runtime::FrameScheduler;
use crate::tree::NodeId;

/// Duration applied when an animation never names one.
const DEFAULT_DURATION: f32 = 0.3;

/// Fluent builder constructing an animation against a keyed view.
pub struct AnimationBuilder<'a> {
    scheduler: &'a mut FrameScheduler,
    node: Option<NodeId>,
    key: Key,
    from: MotionVector,
    to: MotionVector,
    tween: Tween,
    spring: Option<Spring>,
}

impl<'a> AnimationBuilder<'a> {
    /// Reads the start value from the keyed view's current motion state.
    pub fn new(scheduler: &'a mut FrameScheduler, key: Key) -> Self {
        let node = scheduler.arena.node_for_key(&key);
        let from = node
            .map(|id| MotionVector::from_state(&scheduler.arena.get(id).state.motion))
            .unwrap_or(MotionVector::NEUTRAL);
        Self {
            scheduler,
            node,
            key,
            from,
            to: from,
            tween: Tween::new(DEFAULT_DURATION, Ease::OutCubic),
            spring: None,
        }
    }

    /// Sets the target horizontal translation.
    pub fn x(mut self, value: f32) -> Self {
        self.to.x = value;
        self
    }

    /// Sets the target vertical translation.
    pub fn y(mut self, value: f32) -> Self {
        self.to.y = value;
        self
    }

    /// Sets the target rotation in degrees.
    pub fn rotation(mut self, degrees: f32) -> Self {
        self.to.rotation = degrees;
        self
    }

    /// Sets the target uniform scale factor.
    pub fn scale(mut self, value: f32) -> Self {
        self.to.scale_x = value;
        self.to.scale_y = value;
        self
    }

    /// Sets the target opacity multiplier.
    pub fn opacity(mut self, value: f32) -> Self {
        self.to.opacity = value;
        self
    }

    /// Sets the tween duration in seconds.
    pub fn duration(mut self, seconds: f32) -> Self {
        self.tween.duration = seconds;
        self
    }

    /// Sets the easing curve applied to the tween.
    pub fn ease(mut self, ease: Ease) -> Self {
        self.tween.ease = ease;
        self
    }

    /// Switches the animation from a timed tween to physics-based settling.
    pub fn spring(mut self, spring: Spring) -> Self {
        self.spring = Some(spring);
        self
    }

    /// Registers and starts the animation, retargeting any motion already running.
    pub fn play(self) {
        let Some(node) = self.node else {
            return;
        };
        let key = self.key;
        let controller = match self.spring {
            Some(spring) => {
                let mut built = Controller::spring(node, self.from, self.to, spring);
                if let Some(previous) = self.scheduler.motion.controller(&key) {
                    let velocity = previous.velocity();
                    built.adopt_velocity(velocity);
                }
                built
            }
            None => Controller::tween(node, self.from, self.to, self.tween),
        };
        self.scheduler.motion.push(controller.with_key(key));
    }
}
