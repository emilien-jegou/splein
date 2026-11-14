// Responsibility: Demo-effects application entry point with self-hosted live reload.

fn main() {
    riode::App::config()
        .title("Aurora — Effects & Blending Showcase")
        .size(960, 680)
        .background(0x111116)
        .backend_factory(riode_engine_wgpu::create_backend)
        .parse_cli()
        .hot_reload(true)
        .run(demo_effects::Page);
}
