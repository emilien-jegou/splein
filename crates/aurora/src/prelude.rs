// Single responsibility: Convenient top-level re-exports for declarative application authoring.

pub use crate::anim::{
    ControllerHandle, Ease, Millis, PlayState, Repeat, Spring, Transition, Tween,
};
pub use crate::app::{App, AppConfig, AppExtension, FpsLimit, OverlayCaps, OverlayPainter};
// `component` is the #[component] attribute macro re-exported below, so the hand-written
// `dsl::component` function is deliberately left out of the prelude: one name cannot be both.
pub use crate::dsl::{
    custom, group, image, svg, text, view_child, Behavior, ChildSegment, Component, CustomDef,
    Element, GroupDef, IntoElement, Provider, TextDef,
};
pub use crate::foundation::{
    Alignment, Anchor, BlendMode, Color, Direction, Distribution, FalloffCurve, Fill, Gap,
    LayoutPresence, LightEnvironment, Margin, Radius, RenderDriver, Shadow,
    ShadowOffset, ShadowShape, Size, Stroke, StrokeAlign, Transform,
};
pub use crate::hot_reload::{
    ComponentRegistry, ComponentReloadExtension, ComponentRootFn, ComponentVersionFn,
    DevLoop, DevLoopConfig, DevLoopState, DylibReloader, FileWatcher, HotReload, HotReloadStats,
    PollOutcome,
    ReloadReport, RemoteBuildFn, RemoteDirection, RemoteEventFn, RemoteHost, RemoteKind,
    RemoteNode, RemoteReloadExtension, RemoteSize, RemoteSnapshot, RemoteStateDecl,
    RemoteStateFn, RemoteText, RemoteVersionFn, StateWrite, component_abi_fingerprint,
    host_sentinel_address, remote_abi_fingerprint, toolchain_id,
};
pub use crate::input::{ClickInfo, CursorIcon, DragInfo, HitTest, HoverInfo, KeyCode, KeyEvent, PointerButton, PointerEvent, PointerPhase, Propagation};
pub use crate::reactive::{
    ambient_guard, batch, computed, use_frame_clock, use_interval, use_interval_toggle,
    use_playback, use_signal, use_signal_keyed, use_timeout, AmbientGuard, Context, Ctx, Cx,
    Derived, Effect, IntoPropValue, Prop, PropDefault, Provided, Signal, SignalRead,
};
/// Call-site-keyed signal hook: `use_signal!("name", init)` (explicit, move-proof keys).
pub use crate::use_signal;
/// Named derived handle from a plain closure: `derived!(move || signal.get() + 1)`.
pub use crate::derived;

pub use crate::render::RenderBackend;
pub use crate::runtime::{Engine, FrameDiagnostics, FrameReport};
pub use crate::scene::vector::VectorGraphic;
pub use crate::text::{
    FontStyle, LineHeight, SelectionGranularity, TextAlign, TextDecoration, TextOverflow,
    TextSelection,
};
pub use crate::tree::Primitive;
/// Attribute macro marking a function as a component that runs inside the hook scope.
pub use aurora_macros::component;
/// RSX macro building an element from XML-ish syntax over the builder DSL.
pub use aurora_macros::view;
