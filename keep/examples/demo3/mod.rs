// Re-export shim so the hot loop (`examples/demo3/hot.rs`, mounted by the
// generative UI crate) can mount the whole directory with ONE top-level `#[path]`.
//
// The release binary (`demo3/main.rs`) declares its own `mod canvas/hud/...`
// and never touches this file; adding it changes no release behavior. It exists
// because nested inline `mod demo3 { #[path] ... }` does not resolve across
// the tree.
pub mod canvas;
pub mod hud;
pub mod icons;
pub mod inspector;
pub mod sidebar;
pub mod view;
