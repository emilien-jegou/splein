// Single responsibility: Module boundary and re-exports for the declarative DSL.

pub use component::{component, component_scope, Component, IntoElement};
pub use custom::{custom, CustomDef};
pub use diff::ChildOp;
pub use element::Element;
pub use element_ref::ElementRef;
pub use flip::{use_flip, use_follow, use_ref, Trigger};
pub use group::{group, GroupDef};
pub use image::image;
pub use reconciler::reconcile;
pub use svg::svg;
pub use text::{text, TextDef};

pub mod cleanup;
pub mod component;
pub mod custom;
pub mod diff;
pub mod element;
pub mod element_ref;
pub mod flip;
pub mod group;
pub mod image;
pub mod prop;
pub mod reconcile_children;
pub mod reconcile_group;
pub mod reconcile_leaf;
pub mod reconciler;
pub mod svg;
pub mod text;
