// Single responsibility: Facade over aurora-anim-core plus the reactive transition adapter.

pub use aurora_anim_core::{clock, ease, spring, track, tween};
pub use aurora_anim_core::{Clock, Ease, Millis, RectTrack, RectVector, ScalarTrack, Spring, SpringState, Tween};

pub mod transition;

pub use transition::{ControllerHandle, PlayState, Repeat, Transition};
