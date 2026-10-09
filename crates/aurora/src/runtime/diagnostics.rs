// Single responsibility: Structured engine telemetry and frame performance diagnostics payload.

use crate::foundation::ResolvedRect;
use crate::tree::DirtyFlags;
use smallvec::SmallVec;
use std::time::Duration;

/// Complete telemetry snapshot of an executed engine frame.
#[derive(Clone, Debug, Default)]
pub struct FrameDiagnostics {
    pub frame_index: u64,
    pub timings: FrameTimings,
    pub invalidation: InvalidationDiagnostics,
    pub layout: LayoutDiagnostics,
    pub spatial: SpatialDiagnostics,
    pub compile: CompileDiagnostics,
}

#[derive(Clone, Debug, Default)]
pub struct FrameTimings {
    pub total: Duration,
    pub reactivity: Duration,
    pub layout: Duration,
    pub spatial: Duration,
    pub compile: Duration,
    pub raster: Duration,
    pub present: Duration,
}

#[derive(Clone, Debug, Default)]
pub struct InvalidationDiagnostics {
    pub flags: DirtyFlags,
    pub nodes_dirtied: usize,
    pub drain_passes: usize,
    pub paint_passes: usize,
}

#[derive(Clone, Debug, Default)]
pub struct LayoutDiagnostics {
    pub recomputed_nodes: Vec<ResolvedRect>,
    pub cached_nodes: Vec<ResolvedRect>,
    pub boundary_nodes: Vec<ResolvedRect>,
    pub total_nodes: usize,
    pub laid_out: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SpatialDiagnostics {
    pub damaged_rects: SmallVec<[ResolvedRect; 4]>,
    pub removed_rects: Vec<ResolvedRect>,
    pub preserved_canvas_rect: Option<ResolvedRect>,
}

#[derive(Clone, Debug, Default)]
pub struct CompileDiagnostics {
    pub chunks_emitted: usize,
    pub cached_chunks: usize,
    pub stacking_contexts: usize,
    pub commands_emitted: usize,
    pub context_bounds: Vec<ResolvedRect>,
}

/// A read-only snapshot of the engine's internal counters.
///
/// One struct rather than a dozen accessors, because the counters are only interpretable
/// together: a live track count that disagrees with the number of nodes springing is the signature
/// of a stale count, `simulated` means nothing without the time scale it was accumulated under,
/// and "is anything animating" is the conjunction of several of them. Reading them one at a time
/// across a frame boundary is how a mismatch gets missed.
///
/// Taken as a single value, so it cannot be a torn read across a frame.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct EngineDiagnostics {
    /// Scaled delta the last frame produced.
    pub last_delta: f32,
    /// Total animation time simulated so far, in seconds.
    pub simulated: f32,
    /// Time banked toward the next animation quantum, in seconds.
    pub accumulated: f32,
    /// The time scale currently applied to deltas.
    pub time_scale: f32,
    /// Whether a forced single-quantum step is still waiting to run.
    pub animation_step_pending: bool,
}
