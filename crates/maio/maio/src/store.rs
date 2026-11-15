// Responsibility: The .maiodb store: lock-guarded open, register, complete, and snapshot.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};

#[allow(unused_imports)]
use fs4::FileExt;
use maio_core::{AssetId, AssetTask, MAIO_VERSION};

use crate::journal::{self, ArtifactMeta, Record};
use crate::{MaioError, Result};

pub const MANIFEST: &str = "manifest.maiodb";

#[derive(Debug, Default, Clone)]
pub struct Snapshot {
    pub tasks: BTreeMap<AssetId, AssetTask>,
    pub artifacts: BTreeMap<AssetId, ArtifactMeta>,
}

impl Snapshot {
    pub fn pending(&self) -> Vec<(AssetId, &AssetTask)> {
        self.tasks
            .iter()
            .filter(|(id, _)| !self.artifacts.contains_key(id))
            .map(|(id, task)| (*id, task))
            .collect()
    }

    pub fn record_count(&self) -> usize {
        self.tasks.len() + self.artifacts.len()
    }
}

#[derive(Debug, Clone)]
pub struct Maiodb {
    path: PathBuf,
    sync: bool,
}

impl Maiodb {
    pub fn open_workspace(start: &Path) -> Result<Self> {
        let ws_root = Self::find_workspace_root(start).unwrap_or_else(|| start.to_path_buf());
        let dir = ws_root.join("target").join(".maio");
        std::fs::create_dir_all(&dir)
            .map_err(|e| MaioError::Io { context: "create target/.maio dir", source: e })?;
        Ok(Self { path: dir.join(MANIFEST), sync: false })
    }

    pub fn at_manifest(path: PathBuf) -> Self {
        Self { path, sync: false }
    }

    pub fn set_sync(&mut self, sync: bool) {
        self.sync = sync;
    }

    pub fn manifest_path(&self) -> &Path {
        &self.path
    }

    pub fn register(&self, task: &AssetTask) -> Result<AssetId> {
        let id = task.compute_id(MAIO_VERSION);

        let mut file = self.locked_exclusive()?;
        let replayed = journal::replay(&self.path)?;
        if replayed.torn.is_some() {
            journal::truncate(&self.path, replayed.len)?;
        }
        let state = snapshot_of(&replayed);
        if !state.tasks.contains_key(&id) {
            journal::append(&mut file, &Record::Register { id, task: task.clone() }, self.sync)?;
        }
        Ok(id)
    }

    pub fn complete(&self, id: AssetId, meta: &ArtifactMeta) -> Result<()> {
        let mut file = self.locked_exclusive()?;
        journal::append(&mut file, &Record::Complete { id, meta: meta.clone() }, self.sync)?;
        Ok(())
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        if !self.path.exists() {
            return Err(MaioError::ManifestNotFound { path: self.path.clone() });
        }
        let _guard = self.locked_shared()?;
        Ok(snapshot_of(&journal::replay(&self.path)?))
    }

    pub fn clear(&self) -> Result<()> {
        let file = self.locked_exclusive()?;
        file.set_len(0)
            .map_err(|e| MaioError::Io { context: "journal clear", source: e })
    }

    pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
        let mut current = Some(start.to_path_buf());
        while let Some(dir) = current {
            let cargo_toml = dir.join("Cargo.toml");
            if cargo_toml.is_file() {
                if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                    if content.contains("[workspace]") {
                        return Some(dir);
                    }
                }
            }
            current = dir.parent().map(Path::to_path_buf);
        }
        None
    }

    fn locked_exclusive(&self) -> Result<File> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&self.path)
            .map_err(|e| MaioError::Io { context: "open manifest", source: e })?;
        file.lock()
            .map_err(|e| MaioError::Lock(e.to_string()))?;
        Ok(file)
    }

    fn locked_shared(&self) -> Result<File> {
        let file = File::open(&self.path)
            .map_err(|e| MaioError::Io { context: "open manifest for read", source: e })?;
        file.lock_shared()
            .map_err(|e| MaioError::Lock(e.to_string()))?;
        Ok(file)
    }
}

fn snapshot_of(replayed: &journal::Replayed) -> Snapshot {
    let mut state = Snapshot::default();
    for record in &replayed.records {
        match record {
            Record::Register { id, task } => {
                state.tasks.insert(*id, task.clone());
            }
            Record::Complete { id, meta } => {
                state.artifacts.insert(*id, meta.clone());
            }
        }
    }
    state
}
