// Responsibility: Executes Cargo compilation for target dynamic libraries with automatic project discovery.

use super::process::SubprocessHandle;
use super::{read_tail, BuildProcess, BuildRunner};
use bon::Builder;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::instrument;

#[derive(Builder)]
pub struct CargoRunner {
    pub package: String,
    pub example: Option<String>,
    pub manifest_path: Option<PathBuf>,
    #[builder(default = PathBuf::from("."))]
    pub workspace_root: PathBuf,
    pub watch_dir: Option<PathBuf>,
    pub lib_path: Option<PathBuf>,
    #[builder(default)]
    pub extra_args: Vec<String>,
    #[builder(default)]
    pub extra_env: Vec<(String, String)>,
}

impl CargoRunner {
    /// Detects whether the host process is running in debug or release mode.
    fn current_profile() -> &'static str {
        if cfg!(debug_assertions) {
            "dev"
        } else {
            crate::hot_reload::HOT_PROFILE
        }
    }

    /// Automatically discovers package, manifest, watch directory, and target directory.
    pub fn auto_detect() -> Self {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let manifest_path = manifest_dir.join("Cargo.toml");
        let watch_dir = if manifest_dir.join("src").exists() {
            manifest_dir.join("src")
        } else {
            manifest_dir.clone()
        };

        let package = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| {
            manifest_dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "app".into())
        });

        let example = std::env::args()
            .position(|a| a == "--example")
            .and_then(|idx| std::env::args().nth(idx + 1));

        let workspace_root = find_workspace_root(&manifest_dir);
        let profile_dir = if cfg!(debug_assertions) { "debug" } else { crate::hot_reload::HOT_PROFILE };
        let lib_path = workspace_root
            .join("target")
            .join(profile_dir)
            .join(format!("lib{}.so", package.replace('-', "_")));

        Self {
            package,
            example,
            manifest_path: Some(manifest_path),
            workspace_root,
            watch_dir: Some(watch_dir),
            lib_path: Some(lib_path),
            extra_args: Vec::new(),
            extra_env: Vec::new(),
        }
    }

    fn build_command(&self) -> Command {
        let profile = Self::current_profile();
        let mut cmd = Command::new("cargo");
        cmd.arg("build")
            .arg("--lib")
            .arg("--profile")
            .arg(profile)
            .arg("--manifest-path")
            .arg(self.manifest_path());

        for arg in &self.extra_args {
            cmd.arg(arg);
        }

        let mut rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
        if !rustflags.is_empty() {
            rustflags.push(' ');
        }
        rustflags.push_str("-C prefer-dynamic");
        cmd.env("RUSTFLAGS", rustflags);

        if let Some(example) = &self.example {
            cmd.env_remove("AURORA_HOT_EXAMPLE");
            cmd.env("RIODE_HOT_EXAMPLE", example);
        }

        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }

        cmd
    }
}

fn find_workspace_root(start: &Path) -> PathBuf {
    let mut current = start;
    loop {
        if current.join("Cargo.lock").exists() || current.join("target").exists() {
            return current.to_path_buf();
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    start.to_path_buf()
}

impl BuildRunner for CargoRunner {
    #[instrument(skip_all, fields(pkg = %self.package, %reason))]
    fn spawn_build(&self, log_file: &Path, reason: &str) -> std::io::Result<Box<dyn BuildProcess>> {
        let log = std::fs::File::create(log_file)?;
        let child = self
            .build_command()
            .current_dir(&self.workspace_root)
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()?;

        Ok(Box::new(SubprocessHandle::new(child)))
    }

    #[instrument(skip_all, fields(pkg = %self.package, log = %log_path.display()))]
    fn blocking_build(&self, log_path: &Path) -> Result<(), Vec<String>> {
        let _log_file = std::fs::File::create(log_path).map_err(|e| vec![e.to_string()])?;
        let mut child = self
            .build_command()
            .current_dir(&self.workspace_root)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| vec![e.to_string()])?;

        let status = child.wait().map_err(|e| vec![e.to_string()])?;
        if status.success() {
            Ok(())
        } else {
            Err(read_tail(log_path, 20))
        }
    }

    fn lib_path(&self) -> PathBuf {
        self.lib_path.clone().unwrap_or_else(|| {
            let profile_dir = if cfg!(debug_assertions) { "debug" } else { crate::hot_reload::HOT_PROFILE };
            self.workspace_root
                .join("target")
                .join(profile_dir)
                .join(format!("lib{}.so", self.package.replace('-', "_")))
        })
    }

    fn watch_dir(&self) -> PathBuf {
        self.watch_dir.clone().unwrap_or_else(|| {
            let src = self.workspace_root.join("src");
            if src.exists() { src } else { self.workspace_root.clone() }
        })
    }

    fn manifest_path(&self) -> PathBuf {
        self.manifest_path.clone().unwrap_or_else(|| self.workspace_root.join("Cargo.toml"))
    }

    fn target_id(&self) -> &str {
        &self.package
    }
}
