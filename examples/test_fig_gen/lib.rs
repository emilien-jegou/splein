// Responsibility: UI component library exporting the hot-reloadable root entry point for test_fig_gen.

pub mod view;

pub use view::Page;

riode::define_hot_ui!(view::Page);
