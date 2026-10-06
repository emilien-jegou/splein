// Single responsibility: Module boundary and re-exports for the engine runtime domain.

pub use diagnostics::{
    CompileDiagnostics, EngineDiagnostics, FrameDiagnostics, FrameTimings, InvalidationDiagnostics,
    LayoutDiagnostics, SpatialDiagnostics,
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
