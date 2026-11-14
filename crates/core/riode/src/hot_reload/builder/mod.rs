// Responsibility: Core contracts and data structures for invoking UI compilation systems.

pub mod blaze;
pub mod cargo;
pub mod process;
pub mod project;

pub use blaze::BlazeRunner;
pub use cargo::CargoRunner;
pub use process::SubprocessHandle;
pub use project::ProjectLayout;

use std::path::{Path, PathBuf};

/// Outcome of polling an ongoing compilation task.
#[derive(Debug)]
pub enum BuildStatus {
    Running,
    Success,
    Failed { code: Option<i32> },
}

/// Abstract handle to an active compilation job.
pub trait BuildProcess: Send {
    /// Polls whether the build process has completed.
    fn poll_status(&mut self) -> std::io::Result<BuildStatus>;
    /// Aborts or cancels the running build process.
    fn cancel(&mut self) -> std::io::Result<()>;
}

/// Interface for executing compilation tasks (Cargo, Blaze, etc.).
pub trait BuildRunner: Send + 'static {
    /// Spawns a background build logging stderr/stdout into `log_file`.
    fn spawn_build(&self, log_file: &Path, reason: &str) -> std::io::Result<Box<dyn BuildProcess>>;

    /// Executes a blocking boot build before initial window presentation.
    fn blocking_build(&self, log_path: &Path) -> Result<(), Vec<String>>;

    /// Path to the compiled dynamic library target.
    fn lib_path(&self) -> PathBuf;

    /// Directory watched for source modifications.
    fn watch_dir(&self) -> PathBuf;

    /// Manifest file watched for configuration changes.
    fn manifest_path(&self) -> PathBuf;

    /// Target identifier or package name.
    fn target_id(&self) -> &str;
}

/// Reads the last `n` lines of a text file, newest last.
pub fn read_tail(path: &Path, n: usize) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let skip = lines.len().saturating_sub(n);
    lines[skip..].iter().map(|s| s.to_string()).collect()
}
