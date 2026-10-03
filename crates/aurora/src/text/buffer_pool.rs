// Single responsibility: Bounded reuse pool for cosmic-text shaping buffers.

use cosmic_text::Buffer;
use std::collections::HashMap;

/// Maximum retained buffers before the pool is flushed wholesale.
const MAX_BUFFERS: usize = 256;

/// Identity of a reusable buffer: content and metrics, excluding the wrap width.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct BufferKey {
    pub content: Box<str>,
    pub size_bits: u32,
    pub line_height_bits: u32,
    pub weight: u16,
    pub style: u8,
    pub family: Box<str>,
}

/// Retains shaped buffers so a width-only change reuses parsing and line analysis.
#[derive(Default)]
pub(crate) struct BufferPool {
    map: HashMap<BufferKey, Buffer>,
}

impl BufferPool {
    /// Removes and returns the buffer matching the key, if retained.
    pub fn take(&mut self, key: &BufferKey) -> Option<Buffer> {
        self.map.remove(key)
    }

    /// Retains a buffer for a future width change, flushing at capacity.
    pub fn put(&mut self, key: BufferKey, buffer: Buffer) {
        if self.map.len() >= MAX_BUFFERS {
            self.map.clear();
        }
        self.map.insert(key, buffer);
    }
}
