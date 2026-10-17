// Single responsibility: Unified Push/Pop stack evaluation for scene command streams.
//
// Every backend (tiny-skia, wgpu, vello) walks the same `SceneCommand`
// framing commands (`PushTransform`/`PushOffset`/`PushOpacity` and their
// pops) with the same composition semantics. This type is the single home
// of that walk: engines feed it framing commands and read back the current
// absolute transform and opacity, converting to their native transform type
// only at draw time. One walk, one set of semantics, no triple
// re-implementation.

use crate::foundation::{Point, Transform};

/// Evaluated framing state while walking a chunk's command stream.
#[derive(Clone, Debug)]
pub struct SceneStack {
    tx: Transform,
    op: f32,
    tx_frames: Vec<Transform>,
    op_frames: Vec<f32>,
}

impl SceneStack {
    /// Fresh stack over an inherited base (tile offset, chunk placement).
    pub fn new(base_tx: Transform, base_op: f32) -> Self {
        Self {
            tx: base_tx,
            op: base_op,
            tx_frames: Vec::new(),
            op_frames: Vec::new(),
        }
    }

    /// Current absolute transform (base + all pushed frames).
    #[inline(always)]
    pub fn transform(&self) -> Transform {
        self.tx
    }

    /// Current absolute opacity (base × all pushed frames).
    #[inline(always)]
    pub fn opacity(&self) -> f32 {
        self.op
    }

    /// Applies `PushTransform`: composes onto the current transform.
    #[inline(always)]
    pub fn push_transform(&mut self, t: &Transform) {
        self.tx_frames.push(self.tx);
        self.tx = self.tx.multiply(t);
    }

    /// Applies `PushOffset`: translation composition.
    #[inline(always)]
    pub fn push_offset(&mut self, p: Point) {
        self.push_transform(&Transform::from_translation(p.x, p.y));
    }

    /// Applies `PopTransform` / `PopOffset`.
    #[inline(always)]
    pub fn pop_transform(&mut self) {
        if let Some(prev) = self.tx_frames.pop() {
            self.tx = prev;
        }
    }

    /// Applies `PushOpacity`: multiplies onto the current opacity.
    #[inline(always)]
    pub fn push_opacity(&mut self, o: f32) {
        self.op_frames.push(self.op);
        self.op *= o.clamp(0.0, 1.0);
    }

    /// Applies `PopOpacity`.
    #[inline(always)]
    pub fn pop_opacity(&mut self) {
        if let Some(prev) = self.op_frames.pop() {
            self.op = prev;
        }
    }

    /// Depth checkpoints for chunk-scoped execution (truncate back after).
    pub fn checkpoint(&self) -> (usize, usize) {
        (self.tx_frames.len(), self.op_frames.len())
    }

    /// Restores a checkpoint taken with [`Self::checkpoint`].
    pub fn restore(&mut self, checkpoint: (usize, usize)) {
        let (tx_len, op_len) = checkpoint;
        while self.tx_frames.len() > tx_len {
            if let Some(prev) = self.tx_frames.pop() {
                self.tx = prev;
            }
        }
        while self.op_frames.len() > op_len {
            if let Some(prev) = self.op_frames.pop() {
                self.op = prev;
            }
        }
    }
}
