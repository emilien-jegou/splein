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
