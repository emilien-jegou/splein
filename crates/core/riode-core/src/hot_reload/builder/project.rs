// Responsibility: Resolves Cargo project layout, target paths, and active build profiles without hardcoded folder assumptions.

use bon::Builder;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Builder)]
pub struct ProjectLayout {
    pub package: String,
    pub example: Option<String>,
    pub manifest_path: PathBuf,
    pub workspace_root: PathBuf,
    pub watch_dir: PathBuf,
    pub lib_path: PathBuf,
}

impl ProjectLayout {
    pub fn current_profile() -> &'static str {
        if cfg!(debug_assertions) {
            "dev"
        } else {
            crate::hot_reload::HOT_PROFILE
        }
    }

    pub fn profile_dir() -> &'static str {
        if cfg!(debug_assertions) {
            "debug"
        } else {
            crate::hot_reload::HOT_PROFILE
        }
    }

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
        let lib_path = workspace_root
            .join("target")
            .join(Self::profile_dir())
            .join(format!("lib{}.so", package.replace('-', "_")));

        Self {
            package,
            example,
            manifest_path,
            workspace_root,
            watch_dir,
            lib_path,
        }
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
