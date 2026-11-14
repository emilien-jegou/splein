// Responsibility: Declarative UI tree specification, builders, reconciliation dispatch, and motion hooks.

pub use component::{
    component, component_scope, component_scope_named, panic_message, view_child, Component,
    IntoElement, IntoSplice, Provider,
};
pub use custom::{custom, CustomDef};
pub use diff::ChildOp;
pub use element::Element;
pub use element_ref::ElementRef;
pub use flip::{use_flip, use_follow, use_ref, FlipHandle, FollowHandle, Trigger};
pub use scroll::{
    use_scroll, OnScroll, RowFn, ScrollHandle, ScrollView, ScrollViewProps, Scrollbar,
    VirtualScrollView, VirtualScrollViewProps,
};
pub use group::{group, index_keyed_list, keyed_list, Behavior, ChildSegment, GroupElement};
pub use image::{image, ImageElement};
pub use reconciler::reconcile;
pub use svg::svg;
pub use text::{text, TextElement};

pub mod cleanup;
pub use auto_grid::{columns_for, AutoGrid, AutoGridProps};
pub use gesture::{use_gesture, Drag};

pub mod auto_grid;
pub mod component;
pub mod custom;
pub mod diff;
pub mod element;
pub mod element_ref;
pub mod flip;
pub mod gesture;
pub mod group;
pub mod image;
pub mod prop;
pub mod reconcile_children;
pub mod reconcile_group;
pub mod reconcile_leaf;
pub mod reconciler;
pub mod scroll;
pub mod svg;
pub mod text;
