// Single responsibility: Module boundary for time-based interpolation, independent of the UI tree.

pub mod clock;
pub mod ease;
pub mod spring;
pub mod track;
pub mod transition;
pub mod tween;

pub use clock::Clock;
pub use ease::Ease;
pub use spring::{Spring, SpringState};
pub use track::{RectTrack, RectVector, ScalarTrack};
pub use transition::{ControllerHandle, PlayState, Repeat, Transition};
pub use tween::{Millis, Tween};
