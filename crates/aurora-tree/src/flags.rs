// Single responsibility: Dirty state bitflags for orthogonal invalidation tracking.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

/// Bitflags representing invalidation categories across the frame pipeline.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DirtyFlags(pub u8);

impl DirtyFlags {
    pub const NONE: Self = Self(0);
    pub const MEASURE: Self = Self(1 << 0);
    pub const LAYOUT: Self = Self(1 << 1);
    pub const PAINT: Self = Self(1 << 2);
    pub const INTERACTION: Self = Self(1 << 3);
    /// Indicates that at least one descendant within this subtree has dirty flags.
    pub const SUBTREE_DIRTY: Self = Self(1 << 4);

    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline(always)]
    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[inline(always)]
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    #[inline(always)]
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

impl Default for DirtyFlags {
    #[inline(always)]
    fn default() -> Self {
        Self::NONE
    }
}

impl BitOr for DirtyFlags {
    type Output = Self;
    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for DirtyFlags {
    #[inline(always)]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for DirtyFlags {
    type Output = Self;
    #[inline(always)]
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for DirtyFlags {
    #[inline(always)]
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl Not for DirtyFlags {
    type Output = Self;
    #[inline(always)]
    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}
