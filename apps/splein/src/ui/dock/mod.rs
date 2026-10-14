// Declares dock geometry structures and Taffy layout calculation trees.

pub mod geometry;
pub mod layout_tree;

pub use geometry::{Box2D, DockGeometry};
pub use layout_tree::compute_dock_layout;
