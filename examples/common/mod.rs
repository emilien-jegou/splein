// Single responsibility: Shared CLI argument parser and telemetry bootstrap for Aurora examples.

use riode::app::VisualDebugger;
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

/// Maps a user-supplied engine name to a [`RenderBackend`].
///
/// Accepts `tiny-skia` / `skia` / `cpu`, `vello` / `gpu`, and
/// `vello-cpu` / `vello_cpu` (dashes and underscores are interchangeable,
/// case-insensitive). Returns `None` for anything else so typos in unrelated
/// positionals never silently switch the rasterizer.
fn parse_engine_name(raw: &str) -> Option<RenderBackend> {
    match raw.to_lowercase().replace('_', "-").as_str() {
        "tiny-skia" | "tinyskia" | "cpu" => Some(RenderBackend::TinySkia),
        "skia" => Some(RenderBackend::Skia),
        "vello" | "vello-gpu" | "gpu" => Some(RenderBackend::Vello),
        "vello-cpu" | "cpu-vello" | "vellocpu" => Some(RenderBackend::VelloCpu),
        "wgpu" | "raw-wgpu" | "rawgpu" => Some(RenderBackend::Wgpu),
        _ => None,
    }
}

impl ExampleCli {
    /// Parses CLI arguments (`[engine]`, `--gpu`, `--cpu`, `--fps <N>`, `--uncapped`, `--no-hud`).
    ///
    /// The engine can be given positionally, e.g. `cargo run --example demo1 -- vello-cpu`:
    /// `tiny-skia` (default), `vello` (GPU compute), or `vello-cpu` (CPU-staged
    /// Vello). The legacy `--gpu` / `--vello` / `--cpu` / `--skia` flags still
    /// work; an explicit positional engine wins over flags.
    pub fn parse(default_w: u32, default_h: u32) -> Self {
        let args: Vec<String> = std::env::args().collect();

        if args.iter().any(|a| a == "--help" || a == "-h") {
            println!("┌────────────────────────────────────────────────────────┐");
            println!("│                RIODE EXAMPLE RUNNER                   │");
            println!("├────────────────────────────────────────────────────────┤");
            println!("│ [engine]             tiny-skia (default) | vello |     │");
            println!("│                      vello-cpu | wgpu | skia          │");
            println!("│ --gpu / --vello    Use Vello GPU Compute (Metal/VK/D3D)│");
            println!("│ --vello-cpu        Use Vello with CPU-staged coarse    │");
            println!("│ --cpu              Use TinySkia CPU Rasterizer (Def.)  │");
            println!("│ --fps <N>          Cap target FPS (e.g. 144, 240, 360) │");
            println!("│ --uncapped         Disable frame pacer (max benchmark) │");
            println!("│ --no-hud           Disable the on-screen HUD panel     │");
            println!("│ --width <pixels>   Set custom window width             │");
            println!("│ --height <pixels>  Set custom window height            │");
            println!("└────────────────────────────────────────────────────────┘");
            std::process::exit(0);
        }

        // Positional engine first: the first bare arg names the rasterizer.
        let positional = args
            .iter()
            .skip(1)
            .find(|a| !a.starts_with('-'))
            .and_then(|a| parse_engine_name(a));
        let mut backend = positional.unwrap_or(RenderBackend::TinySkia);
        if positional.is_none() {
            if args
                .iter()
                .any(|a| a == "--gpu" || a == "--vello" || a == "--vello-gpu")
            {
                backend = RenderBackend::Vello;
            } else if args.iter().any(|a| a == "--wgpu" || a == "--raw-wgpu") {
                backend = RenderBackend::Wgpu;
            } else if args
                .iter()
                .any(|a| a == "--vello-cpu" || a == "--vello_cpu" || a == "--cpu-vello")
            {
                backend = RenderBackend::VelloCpu;
            }
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

        let engine_name = match backend {
            RenderBackend::TinySkia => "tiny-skia",
            RenderBackend::Skia => "skia",
            RenderBackend::Vello => "vello",
            RenderBackend::VelloCpu => "vello-cpu",
            RenderBackend::Wgpu => "wgpu",
        };
        println!(
            "⚡ Backend: {} | Resolution: {}x{} | FPS: {:?} | HUD: {}",
            engine_name, width, height, fps_limit, show_hud
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

    /// Pre-configures a `MultiBackendConfig` with all five raster backends.
    ///
    /// One shared engine/reactive state. `1-5` select a single backend
    /// window, `Tab`/`F7` cycles. The CLI positional/flags pick the focused
    /// backend.
    pub fn app_config_multi(&self, title: &str) -> riode::app::multibackend::MultiBackendConfig {
        use riode::app::multibackend::MultiBackend;
        let mut debugger = VisualDebugger::new();
        debugger.show_hud = self.show_hud;

        MultiBackend::config([
            Some(riode_tiny_skia::create_backend),
            Some(riode_engine_vello::create_backend),
            Some(riode_engine_vello::create_cpu_backend),
            Some(riode_engine_wgpu::create_backend),
            Some(riode_engine_skia::create_backend),
        ])
        .title(format!(
            "{title} [1-5 engine | Tab/F7 cycle | {}]",
            match self.backend {
                RenderBackend::TinySkia => "tiny-skia",
                RenderBackend::Skia => "skia",
                RenderBackend::Vello => "vello-gpu",
                RenderBackend::VelloCpu => "vello-cpu",
                RenderBackend::Wgpu => "wgpu",
            }
        ))
        .size(self.width, self.height)
        .initial(self.backend)
        .show_hud(self.show_hud)
        .fps_limit(self.fps_limit)
        .extension(debugger)
    }

    /// Pre-configures an `AppConfig` builder with parsed CLI parameters.
    pub fn app_config(&self, title: &str) -> AppConfig {
        let mut debugger = VisualDebugger::new();
        debugger.show_hud = self.show_hud;

        let config = App::config()
            .title(title)
            .size(self.width, self.height)
            .backend(self.backend)
            .fps_limit(self.fps_limit)
            .extension(debugger);
        match self.backend {
            RenderBackend::TinySkia => config.backend_factory(riode_tiny_skia::create_backend),
            RenderBackend::Skia => config.backend_factory(riode_engine_skia::create_backend),
            RenderBackend::Vello => config.backend_factory(riode_engine_vello::create_backend),
            RenderBackend::VelloCpu => config.backend_factory(riode_engine_vello::create_cpu_backend),
            RenderBackend::Wgpu => config.backend_factory(riode_engine_wgpu::create_backend),
        }
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
    multi_backend: bool,
    root: impl Fn() -> E + 'static,
) {
    let cli = ExampleCli::parse(default_w, default_h);
    cli.init_telemetry();
    // `include_bytes!` resolves relative to *this* file, so the font path lives
    // here once instead of once per example.
    const INTER: &[u8] = include_bytes!("../../crates/core/riode/assets/inter-var.ttf");
    if multi_backend {
        cli.app_config_multi(title)
            .font("Inter", INTER)
            .background(background)
            .run(root);
    } else {
        cli.app_config(title)
            .font("Inter", INTER)
            .background(background)
            .run(root);
    }
}
