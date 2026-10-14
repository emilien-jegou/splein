// Single responsibility: Release driver for the shadow playground (see `view.rs`).

#[path = "../common/mod.rs"]
mod common;

mod view;

use aurora::app::switcher::BootCtx;
use aurora::reactive::ReactiveRuntime;
use common::ExampleCli;
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) use view::boot;

fn main() {
    let cli = ExampleCli::parse(view::HOT_WIDTH, view::HOT_HEIGHT);
    cli.init_telemetry();
    // Standalone keeps the classic single-demo runner on the same boot the
    // switchable studio uses: one runtime shared by signals and engine.
    let runtime = Rc::new(RefCell::new(ReactiveRuntime::new()));
    let ctx = BootCtx {
        runtime: Rc::clone(&runtime),
        engine_w: cli.width,
        engine_h: cli.height,
        demos: vec![],
        menu: None,
    };
    let booted = boot(&ctx);
    // `root_fn` is invoked once; Cell moves the prebuilt root into it while
    // still satisfying `Fn`.
    let root = std::cell::Cell::new(Some(booted.root));
    cli.app_config_multi(view::HOT_TITLE)
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(view::HOT_BG)
        .run(move || root.take().expect("root built once"));
}
