// Responsibility: Filters out editor temporary files, scratch locks, and filesystem noise from triggering rebuilds.

use std::path::Path;

/// Determines if a path is editor or filesystem noise that must not trigger a rebuild.
pub fn is_temp_noise(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();

    if name.is_empty() {
        return false;
    }

    name.starts_with('.')
        || name.starts_with('#')
        || name.starts_with(".#")
        || name.ends_with('~')
        || name.ends_with(".tmp")
        || name.ends_with(".temp")
        || name.ends_with(".swp")
        || name.ends_with(".swo")
        || name.ends_with(".bak")
        || name.ends_with('#')
}
