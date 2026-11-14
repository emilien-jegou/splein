// Responsibility: Encapsulates hot-reloaded window chrome attributes and declarative transformations.

use bon::Builder;

/// Declarative window configuration emitted by a hot UI component.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Builder)]
pub struct HotWindowInfo {
    #[builder(default = 800)]
    pub width: u32,
    #[builder(default = 600)]
    pub height: u32,
    #[builder(default = 0xFFFFFF)]
    pub background: u32,
}

impl HotWindowInfo {
    pub const fn fallback() -> Self {
        Self {
            width: 800,
            height: 600,
            background: 0xFFFFFF,
        }
    }

    pub fn with_clamped_size(self, min_w: u32, min_h: u32) -> Self {
        Self {
            width: self.width.max(min_w),
            height: self.height.max(min_h),
            ..self
        }
    }
}

impl Default for HotWindowInfo {
    fn default() -> Self {
        Self::fallback()
    }
}
