// Responsibility: UI component library exporting the hot-reloadable root entry point.

pub mod chrome;
pub mod rows;
pub mod view;

pub use view::Page;

// Emits `riode_component_root` and `riode_hot_version` for dynamic library loading
riode::define_hot_ui!(view::Page);
