// Single responsibility: Convenient top-level re-exports for declarative application authoring.

pub use aurora_anim_core::{Ease, Millis, Spring, Tween};
pub use crate::anim::{ControllerHandle, PlayState, Repeat, Transition};
pub use crate::app::{App, AppConfig, AppExtension, FpsLimit, OverlayCaps, OverlayPainter};
// `component` is the #[component] attribute macro re-exported below, so the hand-written
// `dsl::component` function is deliberately left out of the prelude: one name cannot be both.
pub use crate::dsl::{
    custom, group, image, svg, text, Behavior, Component, CustomDef, Element, GroupDef,
    IntoElement, TextDef,
};
pub use aurora_foundation::{
    Alignment, Anchor, BlendMode, Color, Direction, Distribution, FalloffCurve, Fill, Gap,
    LayoutPresence, LightEnvironment, Margin, Radius, RenderDriver, Shadow,
    ShadowOffset, ShadowShape, Size, Stroke, StrokeAlign, Transform,
};
pub use crate::input::{ClickInfo, CursorIcon, DragInfo, HitTest, HoverInfo, KeyCode, KeyEvent, PointerButton, PointerEvent, PointerPhase, Propagation};
pub use aurora_reactive::{
    ambient_guard, batch, computed, use_frame_clock, use_interval, use_interval_toggle,
    use_playback, use_signal_keyed, use_timeout, AmbientGuard, Cx, Derived, Effect, Signal,
    SignalRead,
};
/// Call-site-keyed signal hook: `use_signal!(init)` or `use_signal!("name", init)`.
pub use aurora_reactive::use_signal;
/// Named derived handle from a plain closure: `derived!(move || signal.get() + 1)`.
pub use aurora_reactive::derived;

pub use crate::render::RenderBackend;
pub use crate::runtime::{Engine, FrameDiagnostics, FrameReport};
pub use crate::scene::vector::VectorGraphic;
pub use aurora_text::{
    FontStyle, LineHeight, SelectionGranularity, TextAlign, TextDecoration, TextOverflow,
    TextSelection,
};
pub use crate::tree::Primitive;
/// Attribute macro marking a function as a component that runs inside the hook scope.
pub use aurora_macros::component;
