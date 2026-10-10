// Hot-reloadable demo1 UI: the shared view, re-executed in place.
//
// The interface itself lives in `demo1_view.rs` next to this file (inside the watched
// `src/` dir, so edits trigger the loop); the standalone `demo1` example includes the
// same file by path. This crate adds only the reload contract: a stable component scope
// (hook state, if the view ever
// grows hooks, plus registry tracking for file-scoped reloads) and the three exported
// entry points the host resolves.
//
// Rules (same as every hot UI crate): `aurora` resolves to the shared instance via the
// alias below (never link aurora statically); same toolchain both sides; keep the
// exported symbols and their signatures exactly as below.

extern crate aurora_shared as aurora;

use aurora::dsl::IntoElement;
use aurora::hot_reload::component_abi_fingerprint;
use aurora::prelude::*;

mod demo1_view;

/// Stable root scope for the demo1 view: hook state (if any is ever added) recalls
/// across reloads, and the registry tracks the file for scoped reload reporting.
#[component(name = "demo1::Page")]
fn Page() -> impl IntoElement {
    demo1_view::build_ui()
}

/// ABI fingerprint the host checks before installing entry points.
#[no_mangle]
pub extern "Rust" fn aurora_hot_version() -> u64 {
    component_abi_fingerprint()
}

/// Rebuilds the interface. Re-run on every reload under the old hook scope.
#[no_mangle]
pub extern "Rust" fn aurora_component_root() -> aurora::dsl::Element {
    Page().into_element()
}
