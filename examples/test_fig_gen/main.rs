// Responsibility: Test-fig-gen application entry point with self-hosted live reload.
use riode::prelude::*;

fn main() {
    App::config()
        .title("Aurora — Figure Generation Test")
        .size(800, 600)
        .background(0x18181B)
        .backend_factory(riode_engine_wgpu::create_backend)
        .parse_cli()
        .hot_reload(true)
        .run(test_fig_gen::Page);
}
