// Single responsibility: Facade over aurora-anim-core and aurora-reactive's transition adapter.

pub use aurora_anim_core::{clock, ease, spring, track, tween};
pub use aurora_anim_core::{Clock, Ease, Millis, RectTrack, RectVector, ScalarTrack, Spring, SpringState, Tween};
pub use aurora_reactive::transition;
pub use aurora_reactive::{ControllerHandle, PlayState, Repeat, Transition};
