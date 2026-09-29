// Single responsibility: Shared CLI argument parser and telemetry bootstrap for Aurora examples.

use aurora::app::VisualDebugger;
use aurora::prelude::*;

#[cfg(feature = "tracy")]
use tracing_subscriber::fmt::format::DefaultFields;

#[cfg(feature = "tracy")]
struct TracyConfig(DefaultFields);

#[cfg(feature = "tracy")]
impl tracing_tracy::Config for TracyConfig {
    type Formatter = DefaultFields;
    fn formatter(&self) -> &Self::Formatter {
        &self.0
    }

    fn on_error(&self, client: &tracing_tracy::client::Client, error: &'static str) {
        client.color_message(error, 0xFF000000, 0);
        eprintln!("tracy-tracing: {error}");
    }

    fn format_fields_in_zone_name(&self) -> bool {
        false
    }
}

/// Command-line argument configuration for example applications.
pub struct ExampleCli {
    /// Active rendering backend chosen via CLI flags.
    pub backend: RenderBackend,
    /// Logical window width in pixels.
    pub width: u32,
    /// Logical window height in pixels.
    pub height: u32,
    /// Whether HUD telemetry panel is enabled on launch.
    pub show_hud: bool,
    /// Frame rate pacing limit policy.
    pub fps_limit: FpsLimit,
}

impl ExampleCli {
    /// Parses CLI arguments (`--gpu`, `--cpu`, `--fps <N>`, `--uncapped`, `--no-hud`).
    pub fn parse(default_w: u32, default_h: u32) -> Self {
        let args: Vec<String> = std::env::args().collect();

        if args.iter().any(|a| a == "--help" || a == "-h") {
            println!("┌────────────────────────────────────────────────────────┐");
            println!("│                AURORA EXAMPLE RUNNER                   │");
            println!("├────────────────────────────────────────────────────────┤");
            println!("│ --gpu / --vello    Use Vello GPU Compute (Metal/VK/D3D)│");
            println!("│ --cpu / --skia     Use TinySkia CPU Rasterizer (Def.)  │");
            println!("│ --fps <N>          Cap target FPS (e.g. 144, 240, 360) │");
            println!("│ --uncapped         Disable frame pacer (max benchmark) │");
            println!("│ --no-hud           Disable the on-screen HUD panel     │");
            println!("│ --width <pixels>   Set custom window width             │");
            println!("│ --height <pixels>  Set custom window height            │");
            println!("└────────────────────────────────────────────────────────┘");
            std::process::exit(0);
        }

        let mut backend = RenderBackend::TinySkia;
        #[cfg(feature = "vello")]
        if args.iter().any(|a| a == "--gpu" || a == "--vello") {
            backend = RenderBackend::Vello;
        }

        let mut width = default_w;
        let mut height = default_h;
        let mut fps_limit = FpsLimit::Auto;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--width" if i + 1 < args.len() => {
                    if let Ok(w) = args[i + 1].parse::<u32>() { width = w; }
                    i += 1;
                }
                "--height" if i + 1 < args.len() => {
                    if let Ok(h) = args[i + 1].parse::<u32>() { height = h; }
                    i += 1;
                }
                "--fps" if i + 1 < args.len() => {
                    if let Ok(f) = args[i + 1].parse::<u32>() { fps_limit = FpsLimit::Custom(f); }
                    i += 1;
                }
                "--uncapped" => {
                    fps_limit = FpsLimit::Uncapped;
                }
                _ => {}
            }
            i += 1;
        }

        let show_hud = !args.iter().any(|a| a == "--no-hud");

        println!(
            "⚡ Backend: {:?} | Resolution: {}x{} | FPS: {:?} | HUD: {}",
            backend, width, height, fps_limit, show_hud
        );

        Self {
            backend,
            width,
            height,
            show_hud,
            fps_limit,
        }
    }

    /// Initializes Tracy profiling layer and stdout logger.
    pub fn init_telemetry(&self) {
        #[cfg(feature = "tracy")]
        {
            use tracing_subscriber::layer::SubscriberExt;
            use tracing_subscriber::util::SubscriberInitExt;
            use tracing_subscriber::Layer;

            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_filter(tracing_subscriber::filter::LevelFilter::INFO);

            let _ = tracing_subscriber::registry()
                .with(fmt_layer)
                .with(tracing_tracy::TracyLayer::new(TracyConfig(Default::default())))
                .try_init();
        }
        #[cfg(not(feature = "tracy"))]
        {
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_target(false)
                .init();
        }
    }

    /// Pre-configures an `AppConfig` builder with parsed CLI parameters.
    pub fn app_config(&self, title: &str) -> AppConfig {
        let mut debugger = VisualDebugger::new();
        debugger.show_hud = self.show_hud;

        App::config()
            .title(title)
            .size(self.width, self.height)
            .backend(self.backend)
            .fps_limit(self.fps_limit)
            .extension(debugger)
    }
}
