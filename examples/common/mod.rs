// Single responsibility: Shared CLI argument parser and telemetry bootstrap for Aurora examples.

use riode::prelude::*;

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
    /// Parses CLI arguments (`[engine]`, `--gpu`, `--cpu`, `--fps <N>`, `--uncapped`, `--no-hud`).
    ///
    /// The engine can be given positionally, e.g. `cargo run --example demo1 -- wgpu`:
    /// `tiny-skia` (default) or `wgpu`. The legacy `--gpu` / `--cpu` flags
    /// still work; an explicit positional engine wins over flags.
    pub fn parse(default_w: u32, default_h: u32) -> Self {
        let args: Vec<String> = std::env::args().collect();

        if args.iter().any(|a| a == "--help" || a == "-h") {
            println!("┌────────────────────────────────────────────────────────┐");
            println!("│                RIODE EXAMPLE RUNNER                   │");
            println!("├────────────────────────────────────────────────────────┤");
            println!("│ [engine]             tiny-skia (default) | wgpu   │");
            println!("│ --gpu              Use Wgpu GPU Rasterizer           │");
            println!("│ --cpu              Use TinySkia CPU Rasterizer (Def.)  │");
            println!("│ --fps <N>          Cap target FPS (e.g. 144, 240, 360) │");
            println!("│ --uncapped         Disable frame pacer (max benchmark) │");
            println!("│ --no-hud           Disable the on-screen HUD panel     │");
            println!("│ --width <pixels>   Set custom window width             │");
            println!("│ --height <pixels>  Set custom window height            │");
            println!("└────────────────────────────────────────────────────────┘");
            std::process::exit(0);
        }

        let mut width = default_w;
        let mut height = default_h;
        let mut fps_limit = FpsLimit::Auto;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--width" if i + 1 < args.len() => {
                    if let Ok(w) = args[i + 1].parse::<u32>() {
                        width = w;
                    }
                    i += 1;
                }
                "--height" if i + 1 < args.len() => {
                    if let Ok(h) = args[i + 1].parse::<u32>() {
                        height = h;
                    }
                    i += 1;
                }
                "--fps" if i + 1 < args.len() => {
                    if let Ok(f) = args[i + 1].parse::<u32>() {
                        fps_limit = FpsLimit::Custom(f);
                    }
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
            "⚡ wgpu | Resolution: {}x{} | FPS: {:?} | HUD: {}",
            width, height, fps_limit, show_hud
        );

        Self {
            width,
            height,
            show_hud,
            fps_limit,
        }
    }
    /// Initializes Tracy profiling layer and stdout logger.
    ///
    /// Honors `RUST_LOG` (e.g. `RUST_LOG=riode::input=debug` to watch text
    /// selection press/drag/release decisions); defaults to info level.
    pub fn init_telemetry(&self) {
        #[cfg(feature = "tracy")]
        {
            use tracing_subscriber::layer::SubscriberExt;
            use tracing_subscriber::util::SubscriberInitExt;
            use tracing_subscriber::Layer;

            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".parse().expect("info parses")),
                );

            let _ = tracing_subscriber::registry()
                .with(fmt_layer)
                .with(tracing_tracy::TracyLayer::new(TracyConfig(
                    Default::default(),
                )))
                .try_init();
        }
        #[cfg(not(feature = "tracy"))]
        {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".parse().expect("info parses")),
                )
                .with_target(false)
                .init();
        }
    }

    /// Pre-configures an `AppConfig` builder with parsed CLI parameters.
    pub fn app_config(&self, title: &str) -> AppConfig {
        App::config()
            .title(title)
            .size(self.width, self.height)
            .fps_limit(self.fps_limit)
            .backend_factory(riode_engine_wgpu::create_backend)
    }
}

/// One-liner bootstrap for the standard demo shape (P14): parse CLI against the
/// window defaults, init telemetry, load Inter, set background, run the root.
///
/// `root` is any element constructor — `view::Page`, `view::build_ui`, or a
/// `move || …` closure. Demos with drivers, extensions, or hand-built runtimes
/// (studio/moving/showcase/demo2/render_bench) keep custom mains.
pub fn example_cli_run<E: IntoElement>(
    title: &str,
    default_w: u32,
    default_h: u32,
    background: impl Into<Color>,
    root: impl Fn() -> E + 'static,
) {
    let cli = ExampleCli::parse(default_w, default_h);
    cli.init_telemetry();
    // `include_bytes!` resolves relative to *this* file, so the font path lives
    // here once instead of once per example.
    const INTER: &[u8] = include_bytes!("../../crates/core/riode/assets/inter-var.ttf");
    cli.app_config(title)
        .font("Inter", INTER)
        .background(background)
        .run(root);
}
