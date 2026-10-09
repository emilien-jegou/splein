// Single responsibility: Pure time-based interpolation math, independent of reactivity.

pub mod clock;
pub mod ease;
pub mod spring;
pub mod track;
pub mod tween;

pub use clock::Clock;
pub use ease::Ease;
pub use spring::{Spring, SpringState};
pub use track::{RectTrack, RectVector, ScalarTrack};
pub use tween::{Millis, Tween};
