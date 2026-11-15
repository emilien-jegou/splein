// Single responsibility: Aggregated per-frame engine output handed back to the caller.

use crate::runtime::diagnostics::FrameDiagnostics;
use crate::runtime::scheduler::FrameStats;
use crate::scene::LayerId;

/// Everything the engine produced for one frame, named so call sites stay readable.
pub struct FrameReport {
    /// Invalidation and layout statistics observed during the frame.
    pub stats: FrameStats,
    /// Layer-backed subtrees invalidated during the frame.
    pub layers: Vec<LayerId>,
    /// Timings and per-pass diagnostics recorded during the frame.
    pub diagnostics: FrameDiagnostics,
}
