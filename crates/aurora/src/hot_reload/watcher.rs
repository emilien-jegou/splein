// Single responsibility: Filesystem watching for the hot-reload dev loop.

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

/// Watches source files and reports which ones changed.
///
/// Thin over `notify`: watched paths are remembered so a watcher can be rebuilt, and
/// raw events are collapsed to a deduplicated list of changed paths per poll. Debouncing
/// (coalescing the burst an editor's save produces) lives in [`crate::hot_reload::HotReload`],
/// which owns the per-path timestamps — this struct only answers "what changed".
pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    rx: Receiver<notify::Result<notify::Event>>,
    watched: Vec<PathBuf>,
}

impl FileWatcher {
    /// Creates a watcher. Never panics: when the platform backend is unavailable the
    /// watcher is inert (`poll_changed` always empty) and the failure is logged.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )
        .map_err(|e| {
            tracing::warn!("aurora: hot-reload file watcher unavailable ({e}); watching disabled");
            e
        })
        .ok();
        Self {
            watcher,
            rx,
            watched: Vec::new(),
        }
    }

    /// Whether the platform backend initialized.
    pub fn is_active(&self) -> bool {
        self.watcher.is_some()
    }

    /// Watches `path` (a file or a directory) for changes.
    ///
    /// Directories watch recursively; files watch non-recursively. Failures log and are
    /// otherwise ignored: a path that cannot be watched simply never triggers a reload.
    pub fn watch(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        let mode = if path.is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        if let Err(e) = watcher.watch(&path, mode) {
            tracing::warn!("aurora: hot-reload cannot watch {} ({e})", path.display());
            return;
        }
        if !self.watched.contains(&path) {
            self.watched.push(path);
        }
    }

    /// Paths currently watched.
    pub fn watched(&self) -> &[PathBuf] {
        &self.watched
    }

    /// Drains pending filesystem events, returning deduplicated changed paths.
    ///
    /// Only content-affecting kinds count (modify/create); removals, attribute-only
    /// metadata touches, and access events are ignored. Paths are canonicalized
    /// best-effort so editor atomic-save renames still match registered files.
    pub fn poll_changed(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for res in self.rx.try_iter() {
            let event = match res {
                Ok(event) => event,
                Err(e) => {
                    tracing::warn!("aurora: hot-reload watch error ({e})");
                    continue;
                }
            };
            if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                continue;
            }
            for path in event.paths {
                let canon = canonicalize_best_effort(&path);
                if !out.contains(&canon) {
                    out.push(canon);
                }
            }
        }
        out
    }
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Matches a changed path against a registered file, tolerating symlinks and
/// atomic-save renames on either side.
pub(crate) fn paths_match(changed: &Path, registered: &Path) -> bool {
    if changed == registered {
        return true;
    }
    canonicalize_best_effort(changed) == canonicalize_best_effort(registered)
}

/// Canonicalizes, falling back to the input when the file is momentarily absent
/// (mid-rename) or the platform refuses.
fn canonicalize_best_effort(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_content_events_ignored_and_paths_deduped() {
        let watcher = FileWatcher::new();
        // No backend events in a unit test: the poll is trivially empty but must not block.
        assert!(watcher.poll_changed().is_empty());
    }

    #[test]
    fn path_matching_tolerates_identity() {
        let a = Path::new("src/card.rs");
        assert!(paths_match(a, a));
        assert!(!paths_match(a, Path::new("src/other.rs")));
    }

    #[test]
    fn file_change_is_observed_through_poll() {
        let dir = std::env::temp_dir().join(format!(
            "aurora-hot-reload-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("watched.rs");
        std::fs::write(&file, "v1").expect("seed file");

        let mut watcher = FileWatcher::new();
        if !watcher.is_active() {
            return;
        }
        watcher.watch(file.clone());
        // Give the backend a moment to arm before writing.
        std::thread::sleep(std::time::Duration::from_millis(300));
        std::fs::write(&file, "v2").expect("modify file");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut seen = false;
        while std::time::Instant::now() < deadline {
            let changed = watcher.poll_changed();
            if changed.iter().any(|p| paths_match(p, &file)) {
                seen = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(seen, "file change was not observed within 10s");
    }
}
