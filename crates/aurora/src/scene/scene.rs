// Single responsibility: Collection of discrete, retained display list chunks.

use crate::scene::chunk::SceneChunk;
use crate::scene::command::SceneCommand;
use std::ops::Deref;

/// Display list composed of discrete, bounded drawing chunks.
#[derive(Default, Debug, Clone)]
pub struct Scene {
    pub chunks: Vec<SceneChunk>,
    /// Pooled command buffers [STAGE0 #10]: Vecs reclaimed from uniquely-owned
    /// (non-retained) chunks on `clear()`, reused for fresh chunks. Capped to
    /// bound peak memory; shared (`Rc`-retained) chunks are never pooled.
    cmd_pool: Vec<Vec<SceneCommand>>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            cmd_pool: Vec::new(),
        }
    }

    /// Hands out a command buffer, reusing a pooled allocation when available.
    pub fn alloc_commands(&mut self) -> Vec<SceneCommand> {
        self.cmd_pool.pop().unwrap_or_default()
    }

    /// Creates a chunk with a pooled command buffer. Prefer over
    /// `SceneChunk::new` on the compile path so fresh chunks reuse peak
    /// allocation instead of malloc'ing per frame.
    pub fn new_chunk(
        &mut self,
        bounds: crate::foundation::ResolvedRect,
        origin: crate::foundation::Point,
    ) -> SceneChunk {
        SceneChunk::with_commands(bounds, origin, self.alloc_commands())
    }

    pub fn push_chunk(&mut self, chunk: SceneChunk) {
        if !chunk.is_empty() {
            self.chunks.push(chunk);
        }
    }

    /// Appends a command to the active chunk, creating one if empty.
    pub fn push(&mut self, cmd: SceneCommand) {
        if self.chunks.is_empty() {
            let buf = self.alloc_commands();
            self.chunks.push(SceneChunk::with_commands(
                crate::foundation::ResolvedRect::ZERO,
                crate::foundation::Point::ZERO,
                buf,
            ));
        }
        self.chunks.last_mut().unwrap().push(cmd);
    }

    pub fn clear(&mut self) {
        // Reclaim uniquely-owned command buffers: `Rc::get_mut` succeeds only
        // when no retained clone (node state, backend caches) references the
        // stream, so pooled Vecs can never alias live chunks. Cap the pool.
        for chunk in self.chunks.drain(..) {
            if chunk.commands.len() < 8 {
                continue;
            }
            let mut rc = chunk.commands;
            if let Some(buf) = std::rc::Rc::get_mut(&mut rc) {
                if self.cmd_pool.len() < 32 {
                    self.cmd_pool.push(std::mem::take(buf));
                }
            }
        }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

impl Deref for Scene {
    type Target = [SceneChunk];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.chunks
    }
}
