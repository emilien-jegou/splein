// Responsibility: Tracks mounted component identities and provides reverse lookups to source files.

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static PENDING_NOTES: RefCell<Vec<BuildNote>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
pub(crate) struct BuildNote {
    pub instance: Box<str>,
    pub path: &'static str,
    pub file: &'static str,
}

pub(crate) fn note_build(instance: Box<str>, path: &'static str, source_file: &'static str) {
    PENDING_NOTES.with(|notes| {
        notes.borrow_mut().push(BuildNote {
            instance,
            path,
            file: source_file,
        })
    });
}

pub(crate) fn take_build_notes() -> Vec<BuildNote> {
    PENDING_NOTES.with(|notes| std::mem::take(&mut *notes.borrow_mut()))
}

pub(crate) fn pending_note_len() -> usize {
    PENDING_NOTES.with(|notes| notes.borrow().len())
}

pub(crate) fn peek_note_instances_since(mark: usize) -> Vec<Box<str>> {
    PENDING_NOTES.with(|notes| {
        let notes = notes.borrow();
        notes
            .get(mark..)
            .unwrap_or(&[])
            .iter()
            .map(|note| note.instance.clone())
            .collect()
    })
}

#[derive(Default)]
pub struct ComponentRegistry {
    mounted: Vec<Box<str>>,
    mounted_set: FxHashSet<Box<str>>,
    file_to_instances: FxHashMap<PathBuf, FxHashSet<Box<str>>>,
}

impl ComponentRegistry {
    pub fn note_built(&mut self, instance: Box<str>, source_file: &'static str) {
        if self.mounted_set.insert(instance.clone()) {
            self.mounted.push(instance.clone());
        }
        self.file_to_instances
            .entry(PathBuf::from(source_file))
            .or_default()
            .insert(instance);
    }

    pub fn contains(&self, instance: &str) -> bool {
        self.mounted_set.contains(instance)
    }

    pub fn mounted(&self) -> &[Box<str>] {
        &self.mounted
    }

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

    pub fn file_entries(&self) -> Vec<(PathBuf, Vec<Box<str>>)> {
        let mut out: Vec<_> = self
            .file_to_instances
            .iter()
            .map(|(f, set)| {
                let mut instances: Vec<_> = set.iter().cloned().collect();
                instances.sort();
                (f.clone(), instances)
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub fn len(&self) -> usize {
        self.mounted.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mounted.is_empty()
    }
}
