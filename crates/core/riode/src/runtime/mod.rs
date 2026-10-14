//! Engine orchestration: the frame, the scheduler, and everything a frame reports.
//!
//! [`Engine`] is the single-point facade the outside world talks to. Inside, each frame
//! runs the same passes in the same order ([`frame::run_frame_passes`]):
//!
//! 1. **Tick** — advance the clock; tickers integrate.
//! 2. **Input** — dispatch queued pointer/key events ([`pointer_input`]).
//! 3. **React** — drain dirty subscribers into node bindings; reconcile structural changes.
//! 4. **Layout** — [`FrameScheduler::commit_frame`]: boundaries or full pass.
//! 5. **Post-layout** — client callbacks that must see fresh geometry (FLIP sampling).
//! 6. **Damage → compile → bounds** — the [`crate::pipeline`] passes.
//! 7. **Report** — [`FrameReport`] with stats + full [`FrameDiagnostics`].
//!
//! # Showcase
//!
//! Headless (tests, benchmarks, tooling) — no window, no backend:
//!
//! ```ignore
//! use riode::prelude::*;
//! use riode::runtime::{Engine, FrameReport};
//!
//! let mut engine = Engine::headless(400, 300);
//! let count = engine.signal("1".to_string());
//! engine.mount(
//!     group().width(400.0).height(300.0)
//!         .children([text(count.clone()).size(16.0)]),
//! );
//! engine.frame();                       // warmup: mount, lay out, paint everything
//!
//! count.set("2".to_string());           // one write…
//! let FrameReport { stats, diagnostics: diag, .. } = engine.frame();
//! assert!(stats.laid_out);              // …re-laid out (text remeasures)…
//! assert_eq!(diag.layout.recomputed_nodes.len(), 1); // …exactly the text's boundary…
//! assert_eq!(diag.spatial.damaged_rects.len(), 1);   // …and repainted exactly once.
//! ```
//!
//! Geometry reads go through [`node_query::NodeQuery`] (`engine.rect(key)` for the
//! laid-out box, `engine.absolute_rect(key)` for the tight projected box, `engine.transform(key)`
//! for the painted transform, `engine.hit_test(x, y)` down the same path as real pointer
//! events). For scripted interaction + pixels, prefer the `Harness`
//! (`riode_tiny_skia::testing`) over driving `Engine` by hand.
//!
//! # Module map
//!
//! | Module | Holds |
//! |--------|-------|
//! | [`engine`] | [`Engine`]: mount/resize/frame/input queueing, arena + scene ownership |
//! | [`scheduler`] | [`FrameScheduler`]: reactive drainage, boundary collection, layout dispatch |
//! | [`frame`] | [`frame::run_frame_passes`]: the pass sequence itself |
//! | [`boundary`] | Layout boundary collection + nested-boundary pruning |
//! | [`node_query`] | [`node_query::NodeQuery`]: key-based geometry reads |
//! | [`pointer_input`] | Event queue → router dispatch bridge |
//! | [`diagnostics`] / [`report`] | [`FrameDiagnostics`], [`FrameReport`], and friends |
//!
//! # Pitfalls
//!
//! - `engine.frame()` on an idle tree does almost nothing (no layout, no damage, zero
//!   commands) — that invariant is pinned by `tests/pipeline_stage5_compile.rs`, and any
//!   commit that breaks it has a performance bug, not a test bug.
//! - `Engine::headless` preloads a fallback font; `Engine::new` does not. Headless text
//!   tests that skip `headless` shape tofu.
//! - `with_arena` hands out the *whole* arena including `get_mut` — it exists for
//!   diagnostics and tests, not for passes. Passes that need writes should grow narrow
//!   `TreeArena` methods instead of reaching through it.

pub use diagnostics::{
    CacheDiagnostics, CompileDiagnostics, EngineDiagnostics, FrameDiagnostics, FrameTimings,
    InvalidationDiagnostics, LayoutDiagnostics, SpatialDiagnostics,
};
pub use engine::Engine;
pub use report::FrameReport;
pub use scheduler::{FrameScheduler, FrameStats};

pub mod boundary;
pub mod diagnostics;
pub mod engine;
pub mod frame;
pub mod node_query;
pub mod pointer_input;
pub mod report;
pub mod scheduler;
