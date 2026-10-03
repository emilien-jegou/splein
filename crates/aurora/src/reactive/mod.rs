// Module boundary and exports for the reactive engine.
//
// This layer is deliberately free of any UI knowledge: it owns signals, derivations,
// effects, and per-frame tickers. Animation adapts to it; the UI tree consumes it.

pub mod batch;
pub mod derived;
pub mod effect;
pub mod hooks;
pub mod id;
pub mod observer;
pub mod prop;
pub mod readable;
pub mod runtime;
pub mod signal;
pub mod ticker;

pub use batch::batch;
pub use derived::Derived;
pub use effect::Effect;
pub use hooks::{
    use_frame_clock, use_interval, use_interval_toggle, use_playback, use_signal, use_timeout, Cx,
};
pub use id::{SignalId, SubscriberId};
pub use observer::Observer;
pub use prop::{computed, DynamicProp, IntoProp};
pub use readable::Readable;
pub use runtime::{run_post_layout, tick, ReactiveRuntime};
pub use signal::Signal;
pub use ticker::Ticker;
