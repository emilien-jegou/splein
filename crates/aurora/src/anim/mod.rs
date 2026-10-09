// Single responsibility: Facade over aurora-anim-core and aurora-reactive's transition adapter.

pub use aurora_reactive::anim::{clock, ease, spring, track, tween};
pub use aurora_reactive::{Clock, Ease, Millis, RectTrack, RectVector, ScalarTrack, Spring, SpringState, Tween};
pub use aurora_reactive::anim::transition;
pub use aurora_reactive::{ControllerHandle, PlayState, Repeat, Transition};
