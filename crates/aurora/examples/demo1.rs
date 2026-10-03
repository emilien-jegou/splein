// Single responsibility: Native OS window presenting the live Paper 1 showcase design.

use aurora::prelude::*;
use common::ExampleCli;

mod common;

const WIDTH: u32 = 500;
const HEIGHT: u32 = 482;

fn main() {
    let cli = ExampleCli::parse(WIDTH, HEIGHT);
    cli.init_telemetry();
    cli.app_config("Aurora — Demo 3 (Optype Font Manager)")
        .font("Inter", include_bytes!("../assets/inter-var.ttf"))
        .background(Color::WHITE)
        .run(build_ui);
}

fn build_ui() -> impl IntoElement {
    group().direction(Direction::Vertical)
        .width(Size::fill()).height(Size::fill())
        .fill(Color::WHITE)
        .alignment(Alignment::Center).distribution(Distribution::Center)
        .children([group().direction(Direction::Vertical)
            .width(500.0)
            .height(Size::fill())
            .margin(Margin::y(41.0))
            .fill(Color::hex(0xCA6D60))
            .alignment(Alignment::Center).distribution(Distribution::Center)
            .children([group().direction(Direction::Horizontal)
                .width(460.0)
                .height(204.0)
                .fill(Color::hex(0x91DF69))
                .gap(16.0)
                .alignment(Alignment::Center).distribution(Distribution::Center)
                .children((
                    group().direction(Direction::Vertical)
                        .width(127.0).height(96.0)
                        .fill(Color::hex(0x4900FF))
                        .alignment(Alignment::Center).distribution(Distribution::Center)
                        .children([text("Hello")
                            .size(16.0)
                            .line_height(20.0)
                            .color(Color::WHITE)]),
                    group()
                        .width(116.0).height(45.0)
                        .fill(Color::hex(0x4900FF))
                        .children([text("bottom-right")
                            .size(12.0)
                            .line_height(16.0)
                            .color(Color::WHITE)
                            .anchor(Anchor::BottomRight)]),
                    group().direction(Direction::Vertical)
                        .width(140.0).height(164.0)
                        .radius(Radius::max())
                        .fill(Color::hex(0x4900FF))
                        .stroke(Stroke::inside(3.0, Color::BLACK))
                        .alignment(Alignment::Center).distribution(Distribution::Center)
                        .children([text("with radius")
                            .size(12.0)
                            .line_height(16.0)
                            .color(Color::WHITE)]),
                ))])])
}
