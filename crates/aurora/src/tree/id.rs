// Generational identifier for retained arena nodes.

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    pub index: u32,
    pub generation: u32,
}

impl NodeId {
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Packs index and generation into a stable 64-bit identity for downstream caches.
    #[inline(always)]
    pub const fn packed(self) -> u64 {
        ((self.generation as u64) << 32) | self.index as u64
    }
}
