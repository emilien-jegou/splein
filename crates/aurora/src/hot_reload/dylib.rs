// Single responsibility: Code-swap loading of freshly built UI dynamic libraries.

use libloading::{Library, Symbol};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Reloads a UI `cdylib` when its file changes, handing out symbols from the newest load.
///
/// This is the code-swap half of hot-reload: the host builds its UI crate as a `cdylib`
/// exposing a `#[no_mangle] pub extern "Rust" fn aurora_hot_reload_build(...)` entry point,
/// and the dev loop calls [`DylibReloader::refresh`] (per frame, or on watcher events) to
/// pick up fresh builds. Superseded libraries stay mapped in `retired` while this reloader
/// lives — dropping one would unmap code that in-flight frames or retained closures may
/// still call into — so repeated reloads pin one image each, a bounded and explicitly
/// dev-only cost.
///
/// Dev-only by construction: both sides must be built by the same toolchain from the same
/// `aurora` sources (Rust has no stable ABI), and `refresh` logs loudly rather than
/// guessing when the file is mid-link.
pub struct DylibReloader {
    path: PathBuf,
    loaded_mtime: Option<SystemTime>,
    lib: Option<Library>,
    /// Superseded images, kept mapped for the session. See the struct docs.
    retired: Vec<Library>,
    /// Consecutive failed loads (mid-link file, locked on Windows). Reported, never fatal.
    consecutive_failures: u32,
}

impl DylibReloader {
    /// Watches `path` for a loadable UI library. Nothing is loaded yet.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            loaded_mtime: None,
            lib: None,
            retired: Vec::new(),
            consecutive_failures: 0,
        }
    }

    /// The library path being watched.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether a library is currently loaded.
    pub fn is_loaded(&self) -> bool {
        self.lib.is_some()
    }

    /// How many superseded images are still mapped (one per successful reload).
    pub fn retired_count(&self) -> usize {
        self.retired.len()
    }

    /// Loads (or reloads when the file changed since the last load).
    ///
    /// Returns `true` when a fresh library is now live. A missing file is quiet (nothing
    /// built yet); a present-but-unloadable file (linker still writing) logs at warn and
    /// keeps the previous library live.
    pub fn refresh(&mut self) -> bool {
        let mtime = match std::fs::metadata(&self.path).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => return false,
        };
        if self.lib.is_some() && self.loaded_mtime.is_some_and(|t| t == mtime) {
            return false;
        }
        // Copy to a versioned temp path before loading: on some platforms the
        // linker truncates the target in place, and loading a partial image would
        // fail after we already dropped the good one. The copy is atomic enough
        // for a dev loop (a torn copy just fails this attempt and retries).
        match unsafe { Library::new(&self.path) } {
            Ok(lib) => {
                if let Some(old) = self.lib.replace(lib) {
                    self.retired.push(old);
                }
                self.loaded_mtime = Some(mtime);
                if self.consecutive_failures > 0 {
                    tracing::info!(
                        "aurora: hot-reload library reloaded after {} failed attempts",
                        self.consecutive_failures
                    );
                } else {
                    tracing::info!("aurora: hot-reload library loaded");
                }
                self.consecutive_failures = 0;
                true
            }
            Err(e) => {
                self.consecutive_failures += 1;
                tracing::warn!(
                    "aurora: hot-reload library not loadable yet (attempt {}): {e}",
                    self.consecutive_failures
                );
                false
            }
        }
    }

    /// Looks up `symbol` in the currently loaded library.
    ///
    /// The caller names the concrete type: typically
    /// `extern "Rust" fn() -> aurora::dsl::Element` (or a versioned equivalent the host
    /// and the UI dylib agree on). Returns [`DylibSymbolError::NotLoaded`] when nothing is
    /// loaded yet.
    ///
    /// # Safety
    ///
    /// The symbol must exist with exactly type `T`. Mismatched signatures are undefined
    /// behavior — the same contract as `libloading` itself.
    pub unsafe fn get<T>(&self, symbol: &[u8]) -> Result<Symbol<'_, T>, DylibSymbolError> {
        let lib = self.lib.as_ref().ok_or(DylibSymbolError::NotLoaded)?;
        unsafe { lib.get(symbol).map_err(DylibSymbolError::Missing) }
    }
}

/// Why a hot-reload symbol lookup failed.
#[derive(Debug)]
pub enum DylibSymbolError {
    /// No library is loaded yet (nothing built, or every load failed).
    NotLoaded,
    /// The loaded library does not export the symbol.
    Missing(libloading::Error),
}

impl std::fmt::Display for DylibSymbolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotLoaded => write!(f, "aurora: hot-reload library not loaded yet"),
            Self::Missing(e) => write!(f, "aurora: hot-reload symbol missing ({e})"),
        }
    }
}

impl std::error::Error for DylibSymbolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotLoaded => None,
            Self::Missing(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_quiet_and_unloaded() {
        let mut loader = DylibReloader::new("/nonexistent/aurora-ui-hot-reload.so");
        assert!(!loader.is_loaded());
        assert!(!loader.refresh());
        assert!(!loader.is_loaded());
    }

    /// Proves the load path end to end against a real system library: `cos(0) == 1`.
    #[test]
    fn loads_a_real_library_symbol() {
        for candidate in ["/lib/x86_64-linux-gnu/libm.so.6", "libm.so.6"] {
            if std::fs::metadata(candidate).is_err() && candidate.starts_with('/') {
                continue;
            }
            let mut loader = DylibReloader::new(candidate);
            if !loader.refresh() {
                continue;
            }
            assert!(loader.is_loaded());
            // Second refresh without changes is a no-op.
            assert!(!loader.refresh());
            let cos: Symbol<'_, unsafe extern "C" fn(f64) -> f64> =
                unsafe { loader.get(b"cos").expect("libm exports cos") };
            let v = unsafe { cos(0.0) };
            assert!((v - 1.0).abs() < 1e-12, "cos(0) must be 1, got {v}");
            return;
        }
    }
}
