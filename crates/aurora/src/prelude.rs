// Single responsibility: Convenient top-level re-exports for declarative application authoring.

pub use crate::anim::{
    ControllerHandle, Ease, Millis, PlayState, Repeat, Spring, Transition, Tween,
};
pub use crate::app::{App, AppConfig, AppExtension, FpsLimit, OverlayCaps, OverlayPainter};
// `component` is the #[component] attribute macro re-exported below, so the hand-written
// `dsl::component` function is deliberately left out of the prelude: one name cannot be both.
pub use crate::dsl::{
    custom, group, svg, text, Component, CustomDef, Element, GroupDef, IntoElement, TextDef,
};
pub use crate::foundation::{
    Alignment, Anchor, Color, Direction, Distribution, Fill, Gap, LayoutPresence, Margin, Radius,
    RenderDriver, Shadow, ShadowKind, Size, Stroke, StrokeAlign, Transform,
};
pub use crate::input::{ClickInfo, DragInfo, HoverInfo, PointerButton, PointerEvent, PointerPhase};
pub use crate::reactive::{
    batch, computed, use_frame_clock, use_interval, use_interval_toggle, use_playback, use_signal,
    use_timeout, Cx, Derived, Effect, Signal,
};

pub use crate::render::RenderBackend;
pub use crate::runtime::{Engine, FrameDiagnostics, FrameReport};
pub use crate::scene::vector::VectorGraphic;
pub use crate::text::{FontStyle, LineHeight, TextAlign, TextDecoration, TextOverflow};
pub use crate::tree::Primitive;
/// Attribute macro marking a function as a component that runs inside the hook scope.
pub use aurora_macros::component;
