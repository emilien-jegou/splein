// Single responsibility: Entrypoint for the playback demo; the page itself is declarative.

#[path = "../common/mod.rs"]
mod common;
#[path = "../common/motion_ui.rs"]
mod motion_ui;

mod view;

use common::ExampleCli;
use motion_ui::*;

pub use view::Page;

pub const WIDTH: u32 = 780;
pub const HEIGHT: u32 = 440;
const DURATION: f32 = 2.4;
const TRAVEL: f32 = 180.0;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();

    cli.app_config("Aurora — Motion 03d · Hover to pause, click to restart")
        .font(
            "Inter",
            include_bytes!("../../crates/aurora/assets/inter-var.ttf"),
        )
        .background(PAGE)
        .run(Page);
}

/// Peak travel of the sliding card, shared with the page so both agree.
pub const TRAVEL_PX: f32 = TRAVEL;
/// Seconds for one end-to-end slide.
pub const SLIDE_SECS: f32 = DURATION;
