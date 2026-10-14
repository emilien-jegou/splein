// Build script: mounts the example's `hot.rs` per `AURORA_HOT_EXAMPLE`.
//
// Every hot-reloadable example owns exactly one `hot.rs`: its module wiring
// plus a single `define_hot_ui! { root, title, size, background }` block
// emitting all four entry points (`riode_hot_version`, `riode_component_root`,
// `riode_hot_window`, `riode_hot_title`). Window facts live once as
// `HOT_*` consts in the shared `view.rs`, referenced by both the release
// `main.rs` and the hot entry block — no mirrors, no metadata files.
//
// `#[path]` attributes inside `hot.rs` resolve relative to its own location,
// so the generated shim is one module declaration. Fails closed (compile
// error naming the example) on a missing `hot.rs` — never silently serves
// the wrong UI.

use std::path::PathBuf;

fn fail(msg: String) -> ! {
    // Emit as a compile error attached to the build script run.
    panic!("riode-hot-gen build.rs: {msg}");
}

fn main() {
    println!("cargo:rerun-if-env-changed=AURORA_HOT_EXAMPLE");
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ws = manifest_dir
        .ancestors()
        .find(|p| {
            std::fs::read_to_string(p.join("Cargo.toml"))
                .map(|s| s.contains("[workspace]"))
                .unwrap_or(false)
        })
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| fail("could not locate workspace root (no ancestor Cargo.toml with [workspace])".into()));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("_gen.rs");
    let name = std::env::var("AURORA_HOT_EXAMPLE").unwrap_or_default();
    if name.is_empty() {
        // No example selected (plain `cargo check --workspace`, IDE runs):
        // emit a stub that can never install (zero fingerprint mismatches the
        // host handshake, so the loop keeps its previous UI with a warning).
        // Loud on purpose — a real run always sets AURORA_HOT_EXAMPLE.
        println!("cargo::warning=AURORA_HOT_EXAMPLE is not set; riode-hot-gen-ui builds as an uninstallable stub (build via `./example.sh <name>`)");
        std::fs::write(
            &out,
            "#[no_mangle]\npub extern \"Rust\" fn riode_hot_version() -> u64 { 0 }\n\n#[no_mangle]\npub extern \"Rust\" fn riode_component_root() -> ::riode::dsl::Element {\nuse ::riode::dsl::IntoElement;\nriode::dsl::group().children([riode::dsl::text(\"no example selected (set AURORA_HOT_EXAMPLE)\").size(16.0)]).into_element()\n}\n",
        )
        .unwrap();
        return;
    }
    if name.contains('/') || name.contains('\\') || name.contains('.') {
        fail(format!("invalid example name {name:?}"));
    }
    let ex_dir = ws.join("examples").join(&name);
    if !ex_dir.is_dir() {
        fail(format!("example dir {} does not exist", ex_dir.display()));
    }
    println!("cargo:rerun-if-changed={}", ex_dir.display());
    let hot = ex_dir.join("hot.rs");
    if !hot.exists() {
        fail(format!(
            "{} has no hot.rs; every hot-reloadable example needs one \
             (wiring + a single `define_hot_ui! {{ ... }}` block)",
            ex_dir.display()
        ));
    }
    println!("cargo:rerun-if-changed={}", hot.display());
    std::fs::write(&out, format!("#[path = \"{}\"]\nmod hot;\n", hot.display())).unwrap();
}
