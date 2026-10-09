// Single responsibility: Reactive signal dependency graph and time-based animation,
// free of any UI knowledge.
//
// `graph` owns signals, derivations, effects, and per-frame tickers; `anim` owns the
// animation math plus the transition adapters that bind it to signals; `hooks` is the
// component-scoped allocation API. The UI tree consumes all three.

pub mod anim;
pub mod graph;
pub mod hooks;

pub use anim::{
    Clock, ControllerHandle, Ease, Millis, PlayState, RectTrack, RectVector, Repeat,
    ScalarTrack, Spring, SpringState, Transition, Tween,
};
pub use graph::{
    ambient_guard, batch, computed, AmbientGuard, Derived, DynamicProp, Effect, IntoProp,
    Observer, ReactiveRuntime, Readable, Signal, SignalId, SignalRead, SubscriberId, Ticker,
    run_post_layout, tick,
};
pub use hooks::{
    use_frame_clock, use_interval, use_interval_toggle, use_playback, use_signal,
    use_signal_keyed, use_timeout, Cx,
};
