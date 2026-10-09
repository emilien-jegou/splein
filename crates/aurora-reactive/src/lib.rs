// Single responsibility: Reactive signal dependency graph and time-based animation,
// free of any UI knowledge.
//
// This layer owns signals, derivations, effects, per-frame tickers, and the animation
// math (clocks, easing, springs, tweens) plus the transition adapters that bind them to
// signals. The UI tree consumes it.

pub mod batch;
pub mod ambient;
pub mod clock;
pub mod derived;
pub mod ease;
pub mod effect;
pub mod hooks;
pub mod id;
pub mod observer;
pub mod prop;
pub mod prop_foundation;
pub mod readable;
pub mod runtime;
pub mod signal;
pub mod spring;
pub mod ticker;
pub mod track;
pub mod transition;
pub mod tween;

pub use ambient::{guard as ambient_guard, AmbientGuard};
pub use batch::batch;
pub use clock::Clock;
pub use derived::Derived;
pub use ease::Ease;
pub use effect::Effect;
pub use hooks::{
    use_frame_clock, use_interval, use_interval_toggle, use_playback, use_signal,
    use_signal_keyed, use_timeout, Cx,
};
pub use id::{SignalId, SubscriberId};
pub use observer::Observer;
pub use prop::{computed, DynamicProp, IntoProp};
pub use readable::{Readable, SignalRead};
pub use runtime::{run_post_layout, tick, ReactiveRuntime};
pub use signal::Signal;
pub use spring::{Spring, SpringState};
pub use ticker::Ticker;
pub use track::{RectTrack, RectVector, ScalarTrack};
pub use transition::{ControllerHandle, PlayState, Repeat, Transition};
pub use tween::{Millis, Tween};
