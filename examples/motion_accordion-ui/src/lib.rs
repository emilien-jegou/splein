// Hot-reloadable motion_accordion UI (dev only): thin shim over
// `examples/motion_accordion/view.rs`.
//
// `aurora` resolves to the shared instance via the alias — never link `aurora`
// statically from this crate, or component bodies panic on a second runtime.
// Edit `examples/motion_accordion/view.rs`, save, and the window rebuilds.
extern crate aurora_shared as aurora;

use aurora::dsl::IntoElement;
use aurora::prelude::*;

#[path = "../../common/motion_ui.rs"]
mod motion_ui;

#[path = "../../motion_accordion/view.rs"]
mod panels;

/// Accordion owns its open-index outside the page; in release `main` builds it
/// on a private runtime, here a hook owns it so it recalls across reloads.
#[component(name = "examples::AccordionRoot")]
fn AccordionRoot() -> impl IntoElement {
    let open = use_signal(Some(0usize));
    panels::Page(open)
}

aurora::define_hot_ui!(AccordionRoot);
