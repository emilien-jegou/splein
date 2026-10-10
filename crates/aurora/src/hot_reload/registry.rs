// Single responsibility: Registry of mounted hot-reloadable components and their source files.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    /// Build notes pushed by `component_scope_named` as components build.
    ///
    /// The wrapper knows its stable identity (`module_path!()::Name`, emitted by
    /// `#[component]`) and its source file (`file!()` at the expansion site), but it
    /// has no handle to the engine — so it files the note here and the engine's
    /// `HotReload` drains it at the start of the next frame.
    static PENDING_NOTES: RefCell<Vec<(Box<str>, &'static str)>> =
        const { RefCell::new(Vec::new()) };
}

/// Files a build note for a component that just ran.
///
/// Called by `component_scope_named` on every build (mount and rebuild alike). Cheap:
/// one small allocation per component build, drained once per frame.
pub(crate) fn note_build(instance: &str, source_file: &'static str) {
    PENDING_NOTES.with(|notes| {
        notes.borrow_mut().push((instance.into(), source_file));
    });
}

/// Takes all pending build notes, leaving the queue empty.
pub(crate) fn take_build_notes() -> Vec<(Box<str>, &'static str)> {
    PENDING_NOTES.with(|notes| std::mem::take(&mut *notes.borrow_mut()))
}

/// Which components are mounted, and which source file each came from.
///
/// Identity is the stable component path (`module_path!()::Name`); the file map answers
/// "which components do I reload when this file changed". Both are best-effort dev-loop
/// data: an unknown instance reloads as a no-op report, never an error.
#[derive(Default)]
pub struct ComponentRegistry {
    /// Every stable component identity seen building, in first-seen order.
    mounted: Vec<Box<str>>,
    /// Whether `mounted` contains `instance` (the `Vec` above keeps order for reports).
    mounted_set: FxHashSet<Box<str>>,
    /// Source file each component identity was expanded in.
    source_file: FxHashMap<Box<str>, &'static str>,
    /// Reverse index: source file to component identities.
    file_to_instances: FxHashMap<PathBuf, FxHashSet<Box<str>>>,
}

impl ComponentRegistry {
    /// Records that `instance` (from `source_file`) built.
    pub fn note_built(&mut self, instance: Box<str>, source_file: &'static str) {
        if self.mounted_set.insert(instance.clone()) {
            self.mounted.push(instance.clone());
        }
        // A component's source file never changes within a process (code-swap ships a new
        // dylib whose host re-registers), so first-seen wins.
        if !self.source_file.contains_key(&instance) {
            self.source_file.insert(instance.clone(), source_file);
        }
        self.file_to_instances
            .entry(PathBuf::from(source_file))
            .or_default()
            .insert(instance);
    }

    /// Every mounted component identity, in first-seen order.
    pub fn mounted(&self) -> &[Box<str>] {
        &self.mounted
    }

    /// Whether `instance` has ever built under this registry.
    pub fn contains(&self, instance: &str) -> bool {
        self.mounted_set.contains(instance)
    }

    /// Every registered (source file, identities) pair, for loose file matching.
    pub fn file_entries(&self) -> Vec<(PathBuf, Vec<Box<str>>)> {
        let mut out: Vec<(PathBuf, Vec<Box<str>>)> = self
            .file_to_instances
            .iter()
            .map(|(file, set)| {
                let mut instances: Vec<Box<str>> = set.iter().cloned().collect();
                instances.sort();
                (file.clone(), instances)
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Component identities expanded from `path`, if any.
    pub fn instances_for_file(&self, path: &Path) -> Vec<Box<str>> {
        self.file_to_instances
            .get(path)
            .map(|set| {
                let mut out: Vec<Box<str>> = set.iter().cloned().collect();
                out.sort();
                out
            })
            .unwrap_or_default()
    }

    /// Number of mounted component identities.
    pub fn len(&self) -> usize {
        self.mounted.len()
    }

    /// Whether anything has built yet.
    pub fn is_empty(&self) -> bool {
        self.mounted.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_and_lookup_round_trip() {
        let mut reg = ComponentRegistry::default();
        reg.note_built("app::Card".into(), "src/card.rs");
        reg.note_built("app::Card".into(), "src/card.rs");
        assert!(reg.contains("app::Card"));
        assert!(!reg.contains("app::Missing"));
        assert_eq!(reg.len(), 1);
        let hits = reg.instances_for_file(Path::new("src/card.rs"));
        assert_eq!(hits, vec![Box::<str>::from("app::Card")]);
        assert!(reg.instances_for_file(Path::new("src/other.rs")).is_empty());
    }
}
