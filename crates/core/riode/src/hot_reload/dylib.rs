// Responsibility: Safe versioned dynamic library snapshotting, loading, and symbol extraction.

use libloading::{Library, Symbol};
use std::path::PathBuf;
use std::time::SystemTime;
use tracing::instrument;

pub struct DylibReloader {
    source_path: PathBuf,
    loaded_mtime: Option<SystemTime>,
    loaded_len: Option<u64>,
    generation: u64,
    staging_dir: PathBuf,
    lib: Option<Library>,
    retired: Vec<Library>,
}

impl DylibReloader {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let staging_dir = std::env::temp_dir().join(format!("riode-hot-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&staging_dir);
        Self {
            source_path: path.into(),
            loaded_mtime: None,
            loaded_len: None,
            generation: 0,
            staging_dir,
            lib: None,
            retired: Vec::new(),
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.lib.is_some()
    }

    pub fn retired_count(&self) -> usize {
        self.retired.len()
    }

    pub fn loaded_mtime(&self) -> Option<SystemTime> {
        self.loaded_mtime
    }

    #[instrument(skip_all, fields(path = %self.source_path.display(), gen = self.generation))]
    pub fn refresh(&mut self) -> bool {
        let (mtime, len) = match std::fs::metadata(&self.source_path) {
            Ok(m) => match (m.modified(), m.len()) {
                (Ok(t), l) => (t, l),
                _ => return false,
            },
            Err(_) => return false,
        };

        if self.lib.is_some() && self.loaded_mtime == Some(mtime) && self.loaded_len == Some(len) {
            return false;
        }

        let gen = self.generation;
        let dest = self.staging_dir.join(format!("libhot_{gen}.so"));
        if let Err(e) = std::fs::copy(&self.path_ref(), &dest) {
            tracing::warn!("riode: failed to snapshot {}: {e}", self.source_path.display());
            return false;
        }

        match unsafe { Library::new(&dest) } {
            Ok(lib) => {
                if let Some(old) = self.lib.replace(lib) {
                    self.retired.push(old);
                }
                if gen >= 2 {
                    let _ = std::fs::remove_file(self.staging_dir.join(format!("libhot_{}.so", gen - 2)));
                }
                self.generation += 1;
                self.loaded_mtime = Some(mtime);
                self.loaded_len = Some(len);
                tracing::info!(
                    "riode: hot-reload library loaded (generation {gen}, {} retired)",
                    self.retired.len()
                );
                true
            }
            Err(e) => {
                let _ = std::fs::remove_file(&dest);
                tracing::warn!("riode: hot-reload library not loadable yet: {e}");
                false
            }
        }
    }

    fn path_ref(&self) -> &PathBuf {
        &self.source_path
    }

    pub unsafe fn get<T>(&self, symbol: &[u8]) -> Result<Symbol<'_, T>, DylibSymbolError> {
        let lib = self.lib.as_ref().ok_or(DylibSymbolError::NotLoaded)?;
        unsafe { lib.get(symbol).map_err(DylibSymbolError::Missing) }
    }

    pub fn bound_address(&self, symbol: &[u8]) -> Result<usize, DylibSymbolError> {
        let sym: Symbol<*mut std::ffi::c_void> = unsafe { self.get(symbol)? };
        Ok(*sym as usize)
    }
}

impl Drop for DylibReloader {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.staging_dir);
    }
}

#[derive(Debug)]
pub enum DylibSymbolError {
    NotLoaded,
    Missing(libloading::Error),
}
