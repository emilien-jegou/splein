// Responsibility: Declarative configuration builder for application windowing, backends, and live hot-reloading.

use super::extension::AppExtension;
use super::runner::{BackendFactory, FpsLimit};
use crate::foundation::Color;
use crate::hot_reload::builder::cargo::CargoRunner;
use crate::hot_reload::builder::BuildRunner;
use crate::hot_reload::ComponentReloadExtension;
use crate::reactive::ReactiveRuntime;
use std::cell::RefCell;
use std::rc::Rc;

/// Embedded default Inter typography asset so applications render text out-of-the-box.
pub const DEFAULT_FONT: &[u8] = include_bytes!("../../assets/inter-var.ttf");

/// Declarative builder for application window initialization and execution.
pub struct AppConfig {
    pub(crate) title: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fullscreen: bool,
    pub(crate) visible: bool,
    pub(crate) fonts: Vec<Vec<u8>>,
    pub(crate) background: Color,
    #[cfg(feature = "desktop")]
    pub(crate) backend_factory: Option<BackendFactory>,
    pub(crate) extensions: Vec<Box<dyn AppExtension>>,
    pub(crate) runtime: Option<Rc<RefCell<ReactiveRuntime>>>,
    pub(crate) fps_limit: FpsLimit,
}

impl AppConfig {
    pub fn new() -> Self {
        Self {
            title: "Aurora Application".into(),
            width: 800,
            height: 600,
            fullscreen: false,
            visible: true,
            fonts: Vec::new(),
            background: Color::WHITE,
            #[cfg(feature = "desktop")]
            backend_factory: None,
            extensions: Vec::new(),
            runtime: None,
            fps_limit: FpsLimit::Auto,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn font(mut self, _name: &str, bytes: &[u8]) -> Self {
        self.fonts.push(bytes.to_vec());
        self
    }

    pub fn size(mut self, w: u32, h: u32) -> Self {
        self.width = w.max(1);
        self.height = h.max(1);
        self
    }

    pub fn background(mut self, color: impl Into<Color>) -> Self {
        self.background = color.into();
        self
    }

    pub fn fps_limit(mut self, limit: impl Into<FpsLimit>) -> Self {
        self.fps_limit = limit.into();
        self
    }

    #[cfg(feature = "desktop")]
    pub fn backend_factory(mut self, factory: BackendFactory) -> Self {
        self.backend_factory = Some(factory);
        self
    }

    pub fn extension(mut self, ext: impl AppExtension + 'static) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    /// Automatically parses CLI parameters (`--width`, `--height`, `--fps`, `--uncapped`, `--no-hot`).
    pub fn parse_cli(mut self) -> Self {
        crate::hot_reload::init_dev_logging();
        let args: Vec<String> = std::env::args().collect();
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--width" if i + 1 < args.len() => {
                    if let Ok(w) = args[i + 1].parse::<u32>() {
                        self.width = w.max(1);
                    }
                    i += 1;
                }
                "--height" if i + 1 < args.len() => {
                    if let Ok(h) = args[i + 1].parse::<u32>() {
                        self.height = h.max(1);
                    }
                    i += 1;
                }
                "--fps" if i + 1 < args.len() => {
                    if let Ok(f) = args[i + 1].parse::<u32>() {
                        self.fps_limit = FpsLimit::Custom(f);
                    }
                    i += 1;
                }
                "--uncapped" => self.fps_limit = FpsLimit::Uncapped,
                _ => {}
            }
            i += 1;
        }
        self
    }

    /// Mounts self-hosted hot-reload. Defaults to enabled unless `--no-hot` is passed.
    pub fn hot_reload(self, enabled: bool) -> Self {
        if !enabled || std::env::args().any(|a| a == "--no-hot") {
            return self;
        }
        crate::hot_reload::init_dev_logging();

        let use_blaze = std::process::Command::new("blaze")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());

        let runner: Box<dyn BuildRunner> = if use_blaze {
            tracing::info!("riode: hot-reload using Blaze build engine");
            Box::new(crate::hot_reload::BlazeRunner::auto_detect())
        } else {
            tracing::info!("riode: hot-reload using Cargo build engine");
            Box::new(CargoRunner::auto_detect())
        };

        self.extension(ComponentReloadExtension::new(runner))
    }

    /// Mounts hot-reload with a custom BuildRunner (e.g. BlazeRunner for Blaze/Bazel builds).
    pub fn hot_reload_with(self, runner: impl BuildRunner + 'static) -> Self {
        if std::env::args().any(|a| a == "--no-hot") {
            return self;
        }
        crate::hot_reload::init_dev_logging();
        self.extension(ComponentReloadExtension::new(Box::new(runner)))
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::new()
    }
}
