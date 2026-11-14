// Responsibility: Demo1 application entry point with self-hosted live reload.
use riode::prelude::*;

fn main() {
    App::config()
        .title("Aurora — Images demo")
        .size(900, 700)
        .background(0xFFFFFF)
        .backend_factory(riode_engine_wgpu::create_backend)
        .parse_cli()
        .hot_reload(true)
        .run(images_demo::Page);
}
