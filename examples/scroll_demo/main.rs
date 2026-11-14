// Responsibility: Scroll demo entry point with automatic self-hosted live reload.

fn main() {
    riode::App::config()
        .title("Scroll · Thumb · Selection")
        .size(800, 600)
        .background(0x0E0E10)
        .backend_factory(riode_engine_wgpu::create_backend)
        .parse_cli()
        .hot_reload(true)
        .run(scroll_demo::Page);
}
