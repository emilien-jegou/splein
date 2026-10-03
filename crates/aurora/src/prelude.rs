// Single responsibility: Convenient top-level re-exports for declarative application authoring.

pub use crate::app::{App, AppConfig, FpsLimit, SurfacePresenter};
pub use crate::dsl::{
    column, component, custom, group, row, svg, text, Component, CustomDef, Element, GroupDef,
    IntoElement, TextDef,
};
pub use crate::foundation::{
    Alignment, Anchor, Color, Direction, Distribution, Fill, Gap, Margin, Radius, Shadow,
    ShadowKind, Size, Stroke, StrokeAlign, Transform,
};
pub use crate::reactive::{batch, computed, Derived, Effect, Signal};
#[cfg(feature = "vello")]
pub use crate::render::VelloRenderer;
pub use crate::render::{RenderBackend, TinySkiaRenderer};
pub use crate::runtime::Engine;
pub use crate::scene::vector::VectorGraphic;
pub use crate::text::{FontStyle, LineHeight};
