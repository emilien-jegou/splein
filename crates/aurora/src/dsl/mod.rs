//! Declarative UI: builders, reconciliation, and motion hooks.
//!
//! You describe what the interface *should look like right now*; the reconciler
//! ([`reconciler::reconcile`]) diffs that description against the retained tree and
//! updates only what changed. A component body runs once per mount — never per frame —
//! and everything dynamic flows through signals captured in property bindings.
//!
//! # The builders
//!
//! | Builder | Makes | Typical modifiers |
//! |---------|-------|-------------------|
//! | [`group()`] | Flex container | `.direction()`, `.width()`, `.fill()`, `.children()`, `.on_click()`, `.shadow()`, `.clip()`, `.key()` |
//! | [`text()`] | Shaped text run | `.size()`, `.color()`, `.align()`, `.selectable()`, `.max_lines()` |
//! | [`image()`] | Raster image | `.size()` (from an [`ImageSource`](crate::scene::ImageSource)) |
//! | [`svg()`] | Vector graphic | `.size()` (anything `Into<VectorGraphic>`: `&str`, `Arc<_>`, …) |
//! | [`custom()`] | Escape hatch | any [`Primitive`](crate::tree::Primitive) you implement |
//!
//! ```ignore
//! use aurora::prelude::*;
//!
//! #[component]
//! fn Card(title: String, open: Signal<bool>) -> impl IntoElement {
//!     group()
//!         .key("card")                       // stable identity across rebuilds
//!         .width(320.0)
//!         .children([
//!             text(title).size(16.0).color(Color::hex(0x111827)),
//!             text(open.map(|o| if o { "Open" } else { "Closed" }))
//!                 .size(12.0)
//!                 .color(Color::hex(0x6B7280)),
//!         ])
//!         .fill(Color::WHITE)
//!         .radius(12.0)
//!         .shadow(8.0)                        // elevation, an (elevation, color) pair, or a Shadow
//!         .on_click(move |_| open.set(!open.get()))
//! }
//! ```
//!
//! # Keeping state across rebuilds
//!
//! A builder that re-runs (a reordered list, a toggled branch) must find its old state.
//! Three mechanisms, in increasing power:
//!
//! 1. **Signals** ([`crate::reactive`]): values that survive anything; prefer these.
//! 2. **Hooks** ([`flip::use_ref`]): per-component-instance memory filed by call order.
//! 3. **FLIP motion** ([`flip::use_flip`], [`flip::use_follow`]): position memory *plus*
//!    the spring that animates the delta — this is how drag-to-reorder lists glide
//!    instead of teleporting (see `examples/motion_reorder.rs`).
//!
//! Identity rules (what "instance" means above): every `#[component]` runs in a
//! scope keyed by `module_path!()::Name`, and `AppConfig::run` mounts the root
//! under a stable `"root"` scope — so ordinal hooks survive remounts while call
//! order is unchanged, and same-name siblings disambiguate by child index.
//! Keyed hooks (`use_signal!("name", …)`) additionally survive reordering and
//! conditional skips; ordinal hooks do not (a skipped hook shifts every later
//! slot — documented hazard, see `tests/hooks_keyed.rs`).
//! [`element_ref::ElementRef`] is the odd one out on purpose: a *pull* handle for asking
//! "where did layout put this?" without subscribing. Reading it never dirties anything,
//! which is what makes measuring safe — but nothing re-runs when the answer changes,
//! so sample it from a ticker, a gesture, or a post-layout callback.
//!
//! # How reconciliation works
//!
//! [`reconciler::reconcile`] walks the new element tree against retained nodes: same key
//! (or same position) reuses the node and updates its style/bindings/handlers;
//! anything else unmounts and remounts. [`diff::ChildOp`] + `reconcile_children` handle
//! keyed lists with minimal moves. `reconcile_group`/`reconcile_leaf` specialize per
//! node kind. Nothing here paints — it only decides *what exists* and marks it dirty.
//!
//! # Pitfalls
//!
//! - Forgetting `.key()` on list rows is the classic bug: state (and FLIP history) is
//!   filed by identity, so position-keyed rows hand each other the wrong springs after
//!   a reorder.
//! - A `text()` fed a fresh `String` every frame re-shapes every frame. Feed it a signal
//!   (or a `Derived`) so shaping is cached until the content actually changes.
//! - `custom()` primitives bypass layout caching — measure honestly in `Primitive::measure`
//!   or every ancestor pays for it.

pub use component::{
    component, component_scope, component_scope_named, panic_message, view_child, Component,
    IntoElement, Provider,
};
pub use custom::{custom, CustomDef};
pub use diff::ChildOp;
pub use element::Element;
pub use element_ref::ElementRef;
pub use flip::{use_flip, use_follow, use_ref, FollowHandle, Trigger};
pub use scroll::{
    use_scroll, OnScroll, RowFn, ScrollHandle, ScrollView, ScrollViewProps, Scrollbar,
    VirtualScrollView, VirtualScrollViewProps,
};
pub use group::{group, index_keyed_list, keyed_list, Behavior, ChildSegment, GroupDef};
pub use image::image;
pub use reconciler::reconcile;
pub use svg::svg;
pub use text::{text, TextDef};

pub mod cleanup;
pub mod auto_grid;
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
pub mod scroll;
pub mod svg;
pub mod text;
