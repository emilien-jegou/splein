// Responsibility: Hot-reload module entry point, developer logging initializers, and public facade re-exports.

pub mod abi;
pub mod builder;
pub mod chrome;
pub mod component;
pub mod contract;
pub mod depinfo;
pub mod dylib;
pub mod events;
pub mod freshness;
pub mod noise;
pub mod orchestrator;
pub mod registry;
pub mod supervisor;
pub mod telemetry;
pub mod watcher;
pub mod window;

pub use abi::component_abi_fingerprint;
pub use builder::blaze::BlazeRunner;
pub use builder::cargo::CargoRunner;
pub use builder::BuildRunner;
pub use chrome::HotWindowInfo;
pub use component::ComponentReloadExtension;
pub use contract::{ContractError, LoadedUiContract};
pub use dylib::DylibReloader;
pub use events::{SupervisorEvent, SupervisorState};
pub use orchestrator::{HotReload, SharedFactory};
pub use supervisor::DevSupervisor;
pub use telemetry::{HotReloadStats, ReloadReport};
pub use watcher::FileWatcher;
pub use window::{ComponentRootFn, ComponentVersionFn, HotTitleFn, HotWindowFn};

/// Release profile used for all hot-reload builds.
pub const HOT_PROFILE: &str = "release";

/// Initializes structured logging for dev workflows.
pub fn init_dev_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".parse().expect("valid filter"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
