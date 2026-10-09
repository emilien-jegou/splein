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

    /// Inverse of [`packed`](Self::packed): restores the generational id from the opaque
    /// identity the reactive layer hands back at publication time.
    #[inline(always)]
    pub const fn unpack(packed: u64) -> Self {
        Self::new(packed as u32, (packed >> 32) as u32)
    }
}
