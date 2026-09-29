// Single responsibility: Module boundary and re-exports for the declarative DSL.

pub use component::{component, Component, IntoElement};
pub use custom::{custom, CustomDef};
pub use diff::ChildOp;
pub use directional::{column, row};
pub use element::Element;
pub use group::{group, GroupDef};
pub use reconciler::reconcile;
pub use crate::runtime::router::SubscriberRouter;
pub use svg::svg;
pub use text::{text, TextDef};

pub mod cleanup;
pub mod component;
pub mod custom;
pub mod diff;
pub mod directional;
pub mod element;
pub mod group;
pub mod prop;
pub mod reconcile_children;
pub mod reconcile_group;
pub mod reconcile_leaf;
pub mod reconciler;
pub mod svg;
pub mod text;
