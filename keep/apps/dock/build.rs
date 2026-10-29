// The dock UI lives in this package's cdylib and resolves `riode::`
// to the single shared runtime (see `extern crate riode_shared as
// riode` in `src/lib.rs`). Emit the `hot_ui` cfg so `src/view.rs`
// picks the shared-image import path (`riode::anodeui::…`). Harmless
// for the host binary: it links the same shared image.
fn main() {
    println!("cargo:rustc-cfg=hot_ui");
    println!("cargo:rerun-if-changed=build.rs");
}
