// Responsibility: Fixture application entry point.

use riode::prelude::*;

fn main() {
    App::config()
        .title("Fixture — <img> override")
        .size(560, 340)
        .background(0x101014)
        .backend_factory(riode_engine_wgpu::create_backend)
        .parse_cli()
        .run(fixture_img_override::Page);
}
