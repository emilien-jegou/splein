// Responsibility: Interacts with platform filesystem APIs to detect modified and created files.

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use tracing::instrument;

pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    rx: Receiver<notify::Result<notify::Event>>,
    watched: Vec<PathBuf>,
}

impl FileWatcher {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )
        .map_err(|e| tracing::warn!("riode: hot-reload watcher unavailable ({e})"))
        .ok();
        Self {
            watcher,
            rx,
            watched: Vec::new(),
        }
    }

    #[instrument(skip_all, fields(path = %path.as_ref().display()))]
    pub fn watch(&mut self, path: impl AsRef<Path>) {
        let path = path.as_ref().to_path_buf();
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        let mode = if path.is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        match watcher.watch(&path, mode) {
            Ok(()) => {
                if !self.watched.contains(&path) {
                    self.watched.push(path);
                }
            }
            Err(e) => tracing::warn!("riode: cannot watch {} ({e})", path.display()),
        }
    }

    pub fn poll_changed(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for res in self.rx.try_iter() {
            let event = match res {
                Ok(ev) => ev,
                Err(e) => {
                    tracing::warn!("riode: watcher event error ({e})");
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

pub fn paths_match(changed: &Path, registered: &Path) -> bool {
    changed == registered
        || canonicalize_best_effort(changed) == canonicalize_best_effort(registered)
}

fn canonicalize_best_effort(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
