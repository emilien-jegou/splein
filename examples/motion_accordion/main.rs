// Single responsibility: Entrypoint for the accordion demo.

#[path = "../common/mod.rs"]
mod common;
#[path = "../common/motion_ui.rs"]
mod motion_ui;

pub mod view;

use std::cell::RefCell;
use std::rc::Rc;

use aurora::reactive::{ReactiveRuntime, Signal};
use common::ExampleCli;
use motion_ui::*;

pub use view::Page;

pub const WIDTH: u32 = 800;
pub const HEIGHT: u32 = 760;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let open = Signal::new(Rc::clone(&runtime), Some(0usize));

    cli.app_config("Aurora — Web 02 · Accordion")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(PAGE)
        .runtime(runtime)
        // The click lives on each header. There is no driver extension and nothing
        // polls the pointer, so an idle frame costs no interaction work at all.
        .run(move || Page(open.clone()));
}
