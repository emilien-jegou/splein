// Responsibility: Defines UI entry-point function signatures, window metadata contracts, and the registration macro.

pub use super::chrome::HotWindowInfo;
use crate::dsl::element::Element;

/// Rebuilds the UI tree from the loaded component dynamic library.
pub type ComponentRootFn = extern "Rust" fn() -> Element;

/// Reports the loaded library's ABI fingerprint.
pub type ComponentVersionFn = extern "Rust" fn() -> u64;

/// Reports declared window chrome dimensions and clear color.
pub type HotWindowFn = extern "Rust" fn() -> HotWindowInfo;

/// Reports the declared window title.
pub type HotTitleFn = extern "Rust" fn() -> &'static str;

/// Exports the standard component hot-reload entry points from a UI crate.
#[macro_export]
macro_rules! define_hot_ui {
    ($root:path) => {
        #[no_mangle]
        pub extern "Rust" fn riode_hot_version() -> u64 {
            $crate::hot_reload::component_abi_fingerprint()
        }

        #[no_mangle]
        pub extern "Rust" fn riode_component_root() -> $crate::dsl::Element {
            use $crate::dsl::IntoElement;
            $root().into_element()
        }
    };
    {
        root: $root:path,
        title: $title:expr,
        size: ($width:expr, $height:expr),
        background: $background:expr $(,)?
    } => {
        $crate::define_hot_ui!($root);

        #[no_mangle]
        pub extern "Rust" fn riode_hot_window() -> $crate::hot_reload::HotWindowInfo {
            $crate::hot_reload::HotWindowInfo {
                width: $width,
                height: $height,
                background: $background,
            }
        }

        #[no_mangle]
        pub extern "Rust" fn riode_hot_title() -> &'static str {
            $title
        }
    };
}
