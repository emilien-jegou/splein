// Single responsibility: Clock, controllers, and published signals driving motion each frame.

use crate::foundation::Key;
use crate::motion::clock::Clock;
use crate::motion::controller::Controller;
use crate::motion::playback::Playback;
use crate::motion::spring::{Spring, SpringState};
use crate::reactive::Signal;

/// A spring whose position is published to a signal on every frame until it settles.
struct Publisher {
    signal: Signal<f32>,
    state: SpringState,
}

/// Frame clock plus the controllers currently animating retained views.
pub struct MotionRuntime {
    clock: Clock,
    controllers: Vec<Controller>,
    publishers: Vec<Publisher>,
}

impl MotionRuntime {
    /// Creates an idle motion runtime holding no controllers.
    pub fn new() -> Self {
        Self {
            clock: Clock::new(),
            controllers: Vec::new(),
            publishers: Vec::new(),
        }
    }

    /// Forces the next frame delta, so headless tests can step time deterministically.
    pub fn force_delta(&mut self, dt: f32) {
        self.clock.force(dt);
    }

    /// Consumes the next frame delta in seconds.
    pub fn tick(&mut self) -> f32 {
        self.clock.tick()
    }

    /// Registers a controller, replacing any animation on the same slot of the same node.
    pub fn push(&mut self, controller: Controller) {
        self.controllers.retain(|running| {
            running.node() != controller.node() || running.target() != controller.target()
        });
        self.controllers.push(controller);
    }

    /// Reads the imperatively created controller for a keyed view.
    pub fn controller(&self, key: &Key) -> Option<&Controller> {
        self.controllers
            .iter()
            .find(|c| c.key() == Some(key))
    }

    /// Mutably reaches the imperatively created controller for a keyed view.
    pub fn controller_mut(&mut self, key: &Key) -> Option<&mut Controller> {
        self.controllers
            .iter_mut()
            .find(|c| c.key() == Some(key))
    }

    /// Mutable slice over every registered controller, for frame stepping.
    pub fn controllers_mut(&mut self) -> &mut [Controller] {
        &mut self.controllers
    }

    /// Drops controllers that settled, were cancelled, or lost their view.
    pub fn retain_running(&mut self) {
        self.controllers.retain(|c| !c.is_finished());
    }

    /// Whether at least one controller still wants frames.
    pub fn is_active(&self) -> bool {
        self.controllers
            .iter()
            .any(|c| c.playback() == Playback::Playing)
            || !self.publishers.is_empty()
    }

    /// Whether nothing is registered at all.
    pub fn is_idle(&self) -> bool {
        self.controllers.is_empty() && self.publishers.is_empty()
    }

    /// Publishes a spring's position to a signal, settling it from `from` toward `to`.
    pub fn publish_spring(&mut self, signal: Signal<f32>, spring: Spring, from: f32, to: f32) {
        self.publishers.push(Publisher {
            signal,
            state: SpringState::new(spring, from, to),
        });
    }

    /// Advances every publisher by `dt`, dropping the ones that settled.
    pub fn step_publishers(&mut self, dt: f32) {
        if self.publishers.is_empty() {
            return;
        }
        for publisher in &mut self.publishers {
            publisher.state.advance(dt);
            publisher.signal.set_if_changed(publisher.state.position);
        }
        self.publishers.retain(|p| !p.state.is_settled());
    }
}

impl Default for MotionRuntime {
    fn default() -> Self {
        Self::new()
    }
}
