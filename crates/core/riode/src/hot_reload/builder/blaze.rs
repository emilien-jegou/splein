// Responsibility: Executes Indexable Blaze compilation for target dynamic libraries.

use super::process::SubprocessHandle;
use super::project::ProjectLayout;
use super::{read_tail, BuildProcess, BuildRunner};
use bon::Builder;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::instrument;

#[derive(Builder)]
pub struct BlazeRunner {
    pub layout: ProjectLayout,
    #[builder(default)]
    pub ignore_pin: bool,
    #[builder(default)]
    pub extra_args: Vec<String>,
    #[builder(default)]
    pub extra_env: Vec<(String, String)>,
}

impl BlazeRunner {
    pub fn auto_detect() -> Self {
        Self::builder().layout(ProjectLayout::auto_detect()).build()
    }

    fn build_command(&self) -> Command {
        let mut cmd = Command::new("blaze");
        if self.ignore_pin {
            cmd.arg("--ignore-pin");
        }
        cmd.arg("build")
            .arg("--lib")
            .arg("--profile")
            .arg(ProjectLayout::current_profile())
            .arg("--manifest-path")
            .arg(&self.layout.manifest_path);

        for arg in &self.extra_args {
            cmd.arg(arg);
        }

        let mut rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
        if !rustflags.is_empty() {
            rustflags.push(' ');
        }
        rustflags.push_str("-C prefer-dynamic");
        cmd.env("RUSTFLAGS", rustflags);

        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        cmd
    }
}

impl BuildRunner for BlazeRunner {
    #[instrument(skip_all, fields(pkg = %self.layout.package, %reason))]
    fn spawn_build(&self, log_file: &Path, reason: &str) -> std::io::Result<Box<dyn BuildProcess>> {
        let log = std::fs::File::create(log_file)?;
        let child = self
            .build_command()
            .current_dir(&self.layout.workspace_root)
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()?;

        Ok(Box::new(SubprocessHandle::new(child)))
    }

    #[instrument(skip_all, fields(pkg = %self.layout.package, log = %log_path.display()))]
    fn blocking_build(&self, log_path: &Path) -> Result<(), Vec<String>> {
        let log_file = std::fs::File::create(log_path).map_err(|e| vec![e.to_string()])?;
        let status = self
            .build_command()
            .current_dir(&self.layout.workspace_root)
            .stdout(Stdio::null())
            .stderr(log_file)
            .status()
            .map_err(|e| vec![e.to_string()])?;

        if status.success() {
            Ok(())
        } else {
            Err(read_tail(log_path, 20))
        }
    }

    fn lib_path(&self) -> PathBuf {
        self.layout.lib_path.clone()
    }

    fn watch_dir(&self) -> PathBuf {
        self.layout.watch_dir.clone()
    }

    fn manifest_path(&self) -> PathBuf {
        self.layout.manifest_path.clone()
    }

    fn target_id(&self) -> &str {
        &self.layout.package
    }
}
