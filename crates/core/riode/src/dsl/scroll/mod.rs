// Architectural goal: Cohesive client-side scroll engine, viewports, hooks, and virtualized scrollers.

pub mod axis;
pub mod clamp;
pub mod handle;
pub mod gesture;
pub mod view;
pub mod virtual_view;

pub use axis::{ScrollAxis, finalize_scroll};
pub use handle::{use_scroll, OnScroll, RowFn, ScrollHandle, Scrollbar};
pub use view::{ScrollView, ScrollViewProps, ScrollViewPropsBuilder};
pub use virtual_view::{VirtualScrollView, VirtualScrollViewProps, VirtualScrollViewPropsBuilder};

pub const DEFAULT_OVERSCROLL: f32 = 512.0;
pub const SCROLL_LINE: f32 = 40.0;
pub(crate) const FLING_DECAY: f32 = 7.0;
pub(crate) const FLING_STOP: f32 = 24.0;
pub(crate) const FLING_MAX: f32 = 4000.0;
pub(crate) const THUMB_MIN: f32 = 24.0;
pub(crate) const THUMB_WIDTH: f32 = 8.0;
pub(crate) const THUMB_INSET: f32 = 4.0;
pub(crate) const THUMB_EDGE: f32 = 6.0;
pub(crate) const THUMB_ALPHA: f32 = 0.5;
pub(crate) const FADE_DELAY: f32 = 1.0;
pub(crate) const FADE_IN: f32 = 0.2;
pub(crate) const FADE_TIME: f32 = 0.3;
pub(crate) const TRACK_ZONE: f32 = 24.0;
pub(crate) const RUBBER_BAND: f32 = 120.0;
pub(crate) const SPRING_K: f32 = 180.0;
pub(crate) const SPRING_C: f32 = 22.0;
pub(crate) const SPRING_X_EPS: f32 = 0.5;
pub(crate) const SPRING_V_EPS: f32 = 30.0;

pub(crate) fn rubber_band(raw: f32, min: f32, max: f32) -> f32 {
    let clamped = raw.clamp(min, max);
    let over = raw - clamped;
    if over == 0.0 {
        clamped
    } else {
        clamped + over / (1.0 + over.abs() / RUBBER_BAND)
    }
}
