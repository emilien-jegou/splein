// Single responsibility: Collection of discrete, retained display list chunks.

use std::ops::Deref;
use crate::scene::chunk::SceneChunk;
use crate::scene::command::SceneCommand;

/// Display list composed of discrete, bounded drawing chunks.
#[derive(Default, Debug, Clone)]
pub struct Scene {
    pub chunks: Vec<SceneChunk>,
}

impl Scene {
    pub fn new() -> Self {
        Self { chunks: Vec::new() }
    }

    pub fn push_chunk(&mut self, chunk: SceneChunk) {
        if !chunk.is_empty() {
            self.chunks.push(chunk);
        }
    }

    /// Appends a command to the active chunk, creating one if empty.
    pub fn push(&mut self, cmd: SceneCommand) {
        if self.chunks.is_empty() {
            self.chunks.push(SceneChunk::default());
        }
        self.chunks.last_mut().unwrap().push(cmd);
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
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
