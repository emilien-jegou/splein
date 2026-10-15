// Single responsibility: Expose the compiling toolchain's identity to the hot-reload
// ABI fingerprint.
//
// The component/remote loops execute dylib code across a `dlopen` boundary where Rust
// has no stable ABI: both images must come from the same toolchain or unwinds,
// layouts, and calling conventions silently disagree (today: SIGABRT). The fingerprint
// functions in `hot_reload` fold this string in, so skew fails the handshake with a
// message instead of aborting. Reruns only when this script or `$RUSTC` changes — the
// value cannot drift inside one build.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RUSTC");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let id = std::process::Command::new(&rustc)
        .arg("-vV")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown-toolchain".to_string());
    println!("cargo:rustc-env=RIODE_TOOLCHAIN_ID={id}");
    // P13: old name kept for one release so mixed-version hosts still handshake.
    println!("cargo:rustc-env=AURORA_TOOLCHAIN_ID={id}");
}
