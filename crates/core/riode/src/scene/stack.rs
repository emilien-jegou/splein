// Single responsibility: Unified Push/Pop stack evaluation for scene command streams.
//
// Every backend walks the same `SceneCommand` framing commands
// (`PushTransform`/`PushOffset` and their pops) with the same composition
// semantics. This type is the single home of that walk: engines feed it
// framing commands and read back the current absolute transform, converting
// to their native transform type only at draw time. One walk, one set of
// semantics, no re-implementation per engine.
//
// Opacity was purged with the legacy paint path: a node's alpha lives inside
// its material stage chain now, so there is nothing left to fold here.

use crate::foundation::{Point, Transform};

/// Evaluated framing state while walking a chunk's command stream.
#[derive(Clone, Debug)]
pub struct SceneStack {
    tx: Transform,
    tx_frames: Vec<Transform>,
}

impl SceneStack {
    /// Fresh stack over an inherited base (tile offset, chunk placement).
    pub fn new(base_tx: Transform) -> Self {
        Self {
            tx: base_tx,
            tx_frames: Vec::new(),
        }
    }

    /// Current absolute transform (base + all pushed frames).
    #[inline(always)]
    pub fn transform(&self) -> Transform {
        self.tx
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

    /// Depth checkpoint for chunk-scoped execution (truncate back after).
    pub fn checkpoint(&self) -> usize {
        self.tx_frames.len()
    }

    /// Restores a checkpoint taken with [`Self::checkpoint`].
    pub fn restore(&mut self, checkpoint: usize) {
        while self.tx_frames.len() > checkpoint {
            if let Some(prev) = self.tx_frames.pop() {
                self.tx = prev;
            }
        }
    }
}
