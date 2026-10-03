// Single responsibility: Module boundary and re-exports for the compositor motion domain.

pub use apply::apply_motion;
pub use builder::AnimationBuilder;
pub use controller::Controller;
pub use ease::Ease;
pub use playback::Playback;
pub use runtime::MotionRuntime;
pub use spring::{Spring, SpringState};
pub use state::MotionState;
pub use target::Target;
pub use timeline::Timeline;
pub use track::SpringTrack;
pub use transition::Transition;
pub use tween::{Millis, Tween};
pub use vector::MotionVector;

mod mode;
mod timing;
pub mod apply;
pub mod builder;
pub mod clock;
pub mod controller;
pub mod ease;
pub mod flip;
pub mod playback;
pub mod runtime;
pub mod spring;
pub mod state;
pub mod step;
pub mod target;
pub mod timeline;
pub mod track;
pub mod transition;
pub mod tween;
pub mod vector;
