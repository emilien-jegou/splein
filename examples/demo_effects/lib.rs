// Responsibility: UI component library exporting the hot-reloadable root entry point for demo_effects.

pub mod view;

pub use view::Page;

riode::define_hot_ui!(view::Page);
