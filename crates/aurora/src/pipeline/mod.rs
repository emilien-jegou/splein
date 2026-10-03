// Single responsibility: Module boundary and re-exports for the frame passes between layout and rasterization.
//
// A frame runs as four ordered domains: `layout` places nodes, `pipeline` turns retained state into a
// display list (damage, then overlap, then compile, then bounds), `render` rasterizes that list, and
// `app` presents the pixels to the OS surface. Passes below are declared in execution order.

pub mod bounds;
pub mod compile;
pub mod damage;
pub mod overlap;

pub use bounds::commit_painted_bounds;
pub use compile::{compile_scene_instrumented, CompileResult};
pub use damage::DamagePlan;
pub use overlap::OverlapPlan;
