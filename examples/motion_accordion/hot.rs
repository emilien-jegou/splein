// Hot entry for `motion_accordion` (dev only): state-owning root plus the
// single `define_hot_ui!` entry block. Mounted by the generative
// `aurora-hot-gen-ui` crate; `#[path]`s resolve relative to this file.
// Window facts live once as `HOT_*` consts in `./view.rs`.
use aurora::dsl::IntoElement;
use aurora::prelude::*;

#[path = "../common/motion_ui.rs"]
mod motion_ui;

#[path = "./view.rs"]
mod panels;

/// Accordion owns its open-index outside the page; in release `main` builds it
/// on a private runtime, here a hook owns it so it recalls across reloads.
#[component(name = "examples::AccordionRoot")]
fn AccordionRoot() -> impl IntoElement {
    let open = use_signal(Some(0usize));
    panels::Page(open)
}

aurora::define_hot_ui! {
    root: AccordionRoot,
    title: panels::HOT_TITLE,
    size: (panels::HOT_WIDTH, panels::HOT_HEIGHT),
    background: panels::HOT_BG,
}
