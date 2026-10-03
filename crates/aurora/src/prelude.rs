// Single responsibility: Convenient top-level re-exports for declarative application authoring.

pub use crate::app::{App, AppConfig, AppExtension, FpsLimit, OverlayCaps, OverlayPainter};
pub use crate::dsl::{
    component, custom, group, svg, text, Component, CustomDef, Element, GroupDef, IntoElement,
    TextDef,
};
pub use crate::foundation::{
    Alignment, Anchor, Color, Direction, Distribution, Fill, Gap, Margin, Radius, Shadow,
    ShadowKind, Size, Stroke, StrokeAlign, Transform,
};
pub use crate::motion::{Ease, Millis, MotionState, Spring, Timeline, Transition, Tween};
pub use crate::reactive::{batch, computed, Derived, Effect, Signal};
#[cfg(feature = "vello")]
pub use crate::render::VelloRenderer;
pub use crate::render::{RenderBackend, TinySkiaRenderer};
pub use crate::runtime::{Engine, FrameDiagnostics, FrameReport};
pub use crate::scene::vector::VectorGraphic;
pub use crate::text::{FontStyle, LineHeight, TextAlign, TextDecoration, TextOverflow};
pub use crate::tree::Primitive;
