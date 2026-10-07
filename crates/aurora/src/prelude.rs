// Single responsibility: Convenient top-level re-exports for declarative application authoring.

pub use crate::anim::{
    ControllerHandle, Ease, Millis, PlayState, Repeat, Spring, Transition, Tween,
};
pub use crate::app::{App, AppConfig, AppExtension, FpsLimit, OverlayCaps, OverlayPainter};
// `component` is the #[component] attribute macro re-exported below, so the hand-written
// `dsl::component` function is deliberately left out of the prelude: one name cannot be both.
pub use crate::dsl::{
    custom, group, image, svg, text, Behavior, Component, CustomDef, Element, GroupDef,
    IntoElement, TextDef,
};
pub use crate::foundation::{
    Alignment, Anchor, Color, Direction, Distribution, Fill, Gap, LayoutPresence, Margin, Radius,
    RenderDriver, Shadow, ShadowKind, Size, Stroke, StrokeAlign, Transform,
};
pub use crate::input::{ClickInfo, DragInfo, HitTest, HoverInfo, PointerButton, PointerEvent, PointerPhase, Propagation};
pub use crate::reactive::{
    ambient_guard, batch, computed, use_frame_clock, use_interval, use_interval_toggle,
    use_playback, use_signal, use_signal_keyed, use_timeout, AmbientGuard, Cx, Derived, Effect,
    Signal, SignalRead,
};
/// Call-site-keyed signal hook: `use_signal!(init)` or `use_signal!("name", init)`.
pub use crate::use_signal;
/// Named derived handle from a plain closure: `derived!(move || signal.get() + 1)`.
pub use crate::derived;

pub use crate::render::RenderBackend;
pub use crate::runtime::{Engine, FrameDiagnostics, FrameReport};
pub use crate::scene::vector::VectorGraphic;
pub use crate::text::{FontStyle, LineHeight, TextAlign, TextDecoration, TextOverflow};
pub use crate::tree::Primitive;
/// Attribute macro marking a function as a component that runs inside the hook scope.
pub use aurora_macros::component;
