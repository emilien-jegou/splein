// Single responsibility: Convenient top-level re-exports for declarative application authoring.

pub use crate::anim::{
    ControllerHandle, Ease, Millis, PlayState, Repeat, Spring, Transition, Tween,
};
pub use crate::app::{App, AppConfig, AppExtension, FpsLimit, OverlayCaps, OverlayPainter};
// `component` is the #[component] attribute macro re-exported below, so the hand-written
// `dsl::component` function is deliberately left out of the prelude: one name cannot be both.
/// Named computed handle (memoized derivation) from a plain closure: `computed!(move || signal.get() + 1)`.
pub use crate::computed;
// User-facing vocabulary only. The repr-builders (`group`/`text`/`image`/`svg`/`custom`)
// and their def types (`GroupDef`/`TextDef`/`ChildSegment`/`CustomDef`) moved to
// `riode::internal` (P10) — spell `<Group>`/`<Text>` in `view!` instead. `Element`/
// `IntoElement`/`IntoSplice`/`Component` stay here: they are the component contract.
pub use crate::dsl::{
    use_gesture, use_scroll, AutoGrid, AutoGridProps, Behavior, Component, Drag, Element,
    IntoElement, OnScroll, Provider, RowFn, ScrollHandle, ScrollView, ScrollViewProps, Scrollbar,
    VirtualScrollView, VirtualScrollViewProps,
};
pub use crate::foundation::{
    Alignment, Anchor, BlendMode, Color, Direction, Distribution, Edges, FalloffCurve, Fill, Gap,
    LayoutPresence, LightEnvironment, Margin, Padding, Radius, RenderDriver, Shadow,
    ShadowOffset, ShadowShape, Size, Stroke, StrokeAlign, Transform,
};
pub use crate::hot_reload::{
    component_abi_fingerprint, host_sentinel_address, remote_abi_fingerprint, toolchain_id,
    ComponentRegistry, ComponentReloadExtension, ComponentRootFn, ComponentVersionFn, DevLoop,
    DevLoopConfig, DevLoopState, DylibReloader, FileWatcher, HotReload, HotReloadStats,
    PollOutcome, ReloadReport, RemoteBuildFn, RemoteDirection, RemoteEventFn, RemoteHost,
    RemoteKind, RemoteNode, RemoteReloadExtension, RemoteSize, RemoteSnapshot, RemoteStateDecl,
    RemoteStateFn, RemoteText, RemoteVersionFn, StateWrite,
};
pub use crate::input::{
    ClickInfo, CursorIcon, DragInfo, HitTest, HoverInfo, KeyCode, KeyEvent, PointerButton,
    PointerEvent, PointerPhase, Propagation, WheelInfo,
};
pub use crate::reactive::{
    ambient_guard, batch, computed, use_effect, use_effect_post_layout, use_frame_clock,
    use_interval, use_interval_toggle, use_playback, use_signal, use_signal_keyed, use_timeout,
    AmbientGuard, Context, Ctx, Cx, Computed, Effect, IntoPropValue, PostLayoutId, Prop,
    PropDefault, Provided, Signal, SignalRead,
};
/// Call-site-keyed signal hook: `use_signal!("name", init)` (explicit, move-proof keys).
pub use crate::use_signal;

pub use crate::render::RenderBackend;
pub use crate::runtime::{Engine, FrameDiagnostics, FrameReport};
pub use crate::scene::vector::VectorGraphic;
pub use crate::text::{
    FontStyle, LineHeight, SelectionGranularity, TextAlign, TextDecoration, TextOverflow,
    TextSelection,
};
pub use crate::tree::Primitive;
/// Attribute macro marking a function as a component that runs inside the hook scope.
pub use riode_macros::component;
/// RSX macro building an element from XML-ish syntax over the builder DSL.
pub use riode_macros::view;
/// Build-time text shaping: `pre_shaped!(font = "..", text = "..", size = ..)`.
pub use riode_macros::pre_shaped;
/// Build-time SVG rasterization: `include_svg!("..", W, H)`.
pub use riode_macros::include_svg;
