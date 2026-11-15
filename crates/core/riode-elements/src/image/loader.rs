// Responsibility: On-demand resolution, dev fast-path loading, and background JIT asset compilation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use maio_core::AssetId;
use riode_core::scene::image::ImageSource;
use super::raster::ImageProcessor;
use super::task::IMAGE_KIND;

static IN_FLIGHT: Mutex<Option<HashSet<AssetId>>> = Mutex::new(None);

pub fn load_asset(id: [u8; 16], width: u32, height: u32, rel_asset_dir: &Path) -> ImageSource {
    let base_dir = resolve_workspace_dir();
    let asset_dir = base_dir.join(rel_asset_dir);
    let asset_id = AssetId::from_bytes(id);
    let path = asset_dir.join(format!("{}.bin", asset_id.as_hex()));

    // 1. If artifact is already baked, read directly
    if path.exists() {
        return ImageSource::from_asset(id, width, height, &asset_dir);
    }

    // 2. Dev mode: spawn background bake and return fast-path source on frame 1
    #[cfg(debug_assertions)]
    {
        spawn_jit_bake(asset_id, asset_dir, base_dir.clone());
        if let Some(src) = dev_load_source(&base_dir, asset_id) {
            return src;
        }
    }

    ImageSource::empty_placeholder(width, height)
}

#[cfg(debug_assertions)]
fn dev_load_source(base_dir: &Path, asset_id: AssetId) -> Option<ImageSource> {
    let db = maio::Maiodb::open_workspace(base_dir).ok()?;
    let snap = db.snapshot().ok()?;
    let task = snap.tasks.get(&asset_id)?;
    let source_path = task.source_path()?;

    let img = image::open(source_path).ok()?;
    let mut rgba = img.into_rgba8();
    for pixel in rgba.chunks_exact_mut(4) {
        let a = pixel[3] as u32;
        pixel[0] = ((pixel[0] as u32 * a) / 255) as u8;
        pixel[1] = ((pixel[1] as u32 * a) / 255) as u8;
        pixel[2] = ((pixel[2] as u32 * a) / 255) as u8;
    }
    tracing::info!(id = %asset_id.short_hex(), "dev fast-path loaded source directly");
    Some(ImageSource::from_rgba8(rgba.width(), rgba.height(), rgba.into_raw()))
}

#[cfg(debug_assertions)]
fn spawn_jit_bake(asset_id: AssetId, asset_dir: PathBuf, base_dir: PathBuf) {
    let mut guard = IN_FLIGHT.lock().unwrap();
    let set = guard.get_or_insert_with(HashSet::new);
    if !set.insert(asset_id) {
        return;
    }

    std::thread::spawn(move || {
        let res = std::panic::catch_unwind(|| {
            let db = match maio::Maiodb::open_workspace(&base_dir) {
                Ok(db) => db,
                Err(e) => {
                    tracing::error!(?e, "JIT bake: failed to open workspace maiodb");
                    return;
                }
            };
            let snap = match db.snapshot() {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(?e, "JIT bake: failed to snapshot maiodb");
                    return;
                }
            };
            let task = match snap.tasks.get(&asset_id) {
                Some(t) => t,
                None => {
                    tracing::error!(id = %asset_id.short_hex(), "JIT bake: task not in manifest");
                    return;
                }
            };

            tracing::info!(id = %asset_id.short_hex(), "background JIT compiling image...");
            let mut registry = maio_compiler::ProcessorRegistry::new();
            registry.register(IMAGE_KIND, ImageProcessor);
            let compiler = maio_compiler::Compiler::new(asset_dir, registry);
            match maio_compiler::compile_and_commit(&db, &compiler, &[(asset_id, task.clone())]) {
                Ok(rep) => {
                    for f in &rep.failed {
                        tracing::error!(id = %f.id.short_hex(), err = %f.error, "JIT bake processor failed");
                    }
                    if rep.is_ok() {
                        tracing::info!(id = %asset_id.short_hex(), "JIT bake completed & committed to maiodb!");
                    }
                }
                Err(e) => tracing::error!(?e, "JIT compile_and_commit error"),
            }
        });

        if let Err(panic) = res {
            tracing::error!(?panic, "JIT bake thread panicked");
        }

        if let Ok(mut guard) = IN_FLIGHT.lock() {
            if let Some(set) = guard.as_mut() {
                set.remove(&asset_id);
            }
        }
    });
}

fn resolve_workspace_dir() -> PathBuf {
    if let Ok(cargo_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(cargo_dir);
        if path.ends_with("examples/images-demo") {
            if let Some(root) = path.parent().and_then(Path::parent) {
                return root.to_path_buf();
            }
        }
        return path;
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
