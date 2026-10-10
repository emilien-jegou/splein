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
    /// Byte length of the loaded image. Compared together with the mtime so a
    /// same-tick rewrite still counts as a change (mtime-only equality misses
    /// those on coarse filesystems).
    loaded_len: Option<u64>,
    /// Generation counter: every load copies the watched file to
    /// `<stem>.hot.<pid>.<gen>.so` beside it and dlopens the copy (see
    /// [`DylibReloader::refresh`]).
    generation: u64,
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
            loaded_len: None,
            generation: 0,
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

    /// Mtime of the currently loaded image, if any. The dev loop compares it
    /// against source freshness to notice edits its watcher missed.
    pub fn loaded_mtime(&self) -> Option<SystemTime> {
        self.loaded_mtime
    }

    /// Loads (or reloads when the file changed since the last load).
    ///
    /// Identity is (mtime, length): mtime-only equality misses same-tick
    /// rewrites on coarse filesystems. A content-identical rebuild still
    /// counts as fresh (new bytes on disk supersede the live image) — the
    /// caller, not this loader, decides whether a rebuild was worthwhile.
    ///
    /// Every load goes through a versioned copy (`<stem>.hot.<pid>.<gen>.so`
    /// beside the watched file), never the watched path itself, for two
    /// load-bearing reasons:
    ///
    /// - The linker rewrites the target in place, and `dlopen` on an unchanged
    ///   path returns the *already-mapped* image (refcount bump, no remap).
    ///   Loading the path directly therefore executes stale code with fresh
    ///   mtimes forever: retire counts advance, rebuilds "run", entry addresses
    ///   stay identical, and nothing on screen ever changes.
    /// - A mid-link (torn) file must fail this attempt without touching the
    ///   live mapping; a torn *copy* just fails and retries.
    ///
    /// Copies carry the process id so two hosts looping the same package never
    /// share a generation namespace. Generations older than the previous one
    /// are unlinked on each successful load (mappings persist via the retired
    /// images regardless — unlinking a mapped file is safe); a killed host may
    /// leave orphans, bounded tiny dev-only cruft like the retired mappings.
    ///
    /// Returns `true` when a fresh library is now live. A missing file is quiet (nothing
    /// built yet); a present-but-unloadable file (linker still writing) logs at warn and
    /// keeps the previous library live.
    pub fn refresh(&mut self) -> bool {
        let (mtime, len) = match std::fs::metadata(&self.path) {
            Ok(m) => match (m.modified(), m.len()) {
                (Ok(t), len) => (t, len),
                _ => return false,
            },
            Err(_) => return false,
        };
        if self.lib.is_some()
            && self.loaded_mtime == Some(mtime)
            && self.loaded_len == Some(len)
        {
            return false;
        }
        let gen = self.generation;
        let dest = generation_path(&self.path, gen);
        if let Err(e) = std::fs::copy(&self.path, &dest) {
            tracing::warn!(
                "aurora: hot-reload cannot snapshot {} ({e}); keeping previous library",
                self.path.display()
            );
            return false;
        }
        match unsafe { Library::new(&dest) } {
            Ok(lib) => {
                if let Some(old) = self.lib.replace(lib) {
                    self.retired.push(old);
                }
                // Generations older than the previous one are dead: on a
                // single-threaded engine nothing calls superseded code after a
                // swap, so their files can go (mappings persist regardless).
                if gen >= 2 {
                    let stale = generation_path(&self.path, gen - 2);
                    if std::fs::remove_file(&stale).is_err() {
                        tracing::debug!(
                            "aurora: hot-reload could not remove superseded {}",
                            stale.display()
                        );
                    }
                }
                self.generation += 1;
                self.loaded_mtime = Some(mtime);
                self.loaded_len = Some(len);
                if self.consecutive_failures > 0 {
                    tracing::info!(
                        "aurora: hot-reload library reloaded after {} failed attempts",
                        self.consecutive_failures
                    );
                } else {
                    tracing::info!(
                        "aurora: hot-reload library loaded (generation {gen})"
                    );
                }
                self.consecutive_failures = 0;
                true
            }
            Err(e) => {
                // Torn snapshot: drop it so retries copy fresh bytes next poll.
                let _ = std::fs::remove_file(&dest);
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

    /// Address the loaded image binds `symbol` to, for single-instance checks.
    ///
    /// Used with `aurora_runtime_sentinel`: when the image resolved its aurora imports
    /// to this process's live runtime, the bound address equals the host's own
    /// sentinel address. A mismatch means a second runtime image got mapped (a
    /// statically-linked driver) and component bodies would panic on an empty scope
    /// stack — the caller refuses to install entry points instead.
    pub fn bound_address(&self, symbol: &[u8]) -> Result<usize, DylibSymbolError> {
        let sym: Symbol<*mut std::ffi::c_void> = unsafe {
            // SAFETY: only the address is read, never called through this type.
            self.get(symbol)?
        };
        Ok(*sym as usize)
    }
}

/// Copy path for `generation` of the watched library: same directory (so
/// `DT_NEEDED` resolution behaves identically), unique per process and generation
/// so no `dlopen` ever aliases an already-mapped image. Keeps the `.so` extension
/// so tooling treats the snapshot normally.
fn generation_path(watched: &Path, generation: u64) -> PathBuf {
    let stem = watched
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    watched.with_file_name(format!(
        "{stem}.hot.{}.{generation}.so",
        std::process::id()
    ))
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

    /// Every load snapshots to a unique per-generation copy (never the watched path:
    /// `dlopen` on an unchanged path returns the already-mapped image, serving stale
    /// code with fresh mtimes forever). Copies older than the previous generation are
    /// unlinked on each successful load.
    #[test]
    fn reloads_snapshot_per_generation_and_gc_old() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("aurora-hot-gen-{stamp}"));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let source = ["/lib/x86_64-linux-gnu/libm.so.6", "libm.so.6"]
            .into_iter()
            .find(|c| std::fs::metadata(c).is_ok());
        let Some(source) = source else {
            return;
        };
        let watched = dir.join("libwatched.so");
        std::fs::copy(source, &watched).expect("seed watched copy");
        let copy_of = |gen: u64| super::generation_path(&watched, gen);

        let mut loader = DylibReloader::new(&watched);
        assert!(loader.refresh(), "first load snapshots generation 0");
        assert!(copy_of(0).exists(), "gen-0 copy on disk");
        // Rewrite with different bytes (length change counts even on coarse mtimes).
        let mut bytes = std::fs::read(&watched).expect("read watched");
        bytes.push(0);
        std::fs::write(&watched, &bytes).expect("rewrite watched");
        assert!(loader.refresh(), "second load snapshots generation 1");
        assert!(copy_of(1).exists(), "gen-1 copy on disk");
        assert_eq!(loader.retired_count(), 1);
        bytes.push(0);
        std::fs::write(&watched, &bytes).expect("rewrite watched again");
        assert!(loader.refresh(), "third load snapshots generation 2");
        assert!(copy_of(2).exists(), "gen-2 copy on disk");
        assert!(
            !copy_of(0).exists(),
            "copies older than the previous generation are unlinked"
        );
        assert!(copy_of(1).exists(), "previous generation stays mapped");
        assert_eq!(loader.retired_count(), 2);
        // The loaded image still resolves: snapshots are real libraries.
        let cos: Symbol<'_, unsafe extern "C" fn(f64) -> f64> =
            unsafe { loader.get(b"cos").expect("libm exports cos") };
        assert!((unsafe { cos(0.0) } - 1.0).abs() < 1e-12);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
