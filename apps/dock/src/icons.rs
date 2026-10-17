// Single responsibility: dock icon map (build-time resampled images).
//
// `include_image!` paths resolve against CARGO_MANIFEST_DIR of the crate
// being built. This module compiles inside the `dock` package itself
// (both the overlay binary's crate graph and this hot cdylib), so paths
// are spelled from the package root to its own `assets/` store.

use riode::scene::ImageSource;
use std::collections::HashMap;
use std::sync::OnceLock;

macro_rules! icon {
    ($path:literal) => {
        riode::prelude::include_image!($path, 56, 56)
    };
}

/// All registry icons, preprocessed at build time (Lanczos3-resampled to 32×32).
pub fn icon_map() -> &'static HashMap<&'static str, ImageSource> {
    static MAP: OnceLock<HashMap<&'static str, ImageSource>> = OnceLock::new();
    MAP.get_or_init(|| {
        HashMap::from([
            ("finder", icon!("assets/icons/finder.png")),
            ("safari", icon!("assets/icons/safari.png")),
            ("notes", icon!("assets/icons/Notes.png")),
            ("photos", icon!("assets/icons/photos.png")),
            ("terminal", icon!("assets/icons/terminal.png")),
            ("vscode", icon!("assets/icons/microsoft_visual_studio_code.png")),
            ("contact", icon!("assets/icons/contact.webp")),
            ("resume", icon!("assets/icons/pdf.png")),
            ("settings", icon!("assets/icons/settings.png")),
            ("mail", icon!("assets/icons/mail.webp")),
            ("github", icon!("assets/icons/github.webp")),
            ("telegram", icon!("assets/icons/telegram.png")),
            ("trash", icon!("assets/icons/trash.png")),
        ])
    })
}
