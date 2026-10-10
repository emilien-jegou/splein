#!/usr/bin/env bash
# Demo 1 hot-reload loop: builds the launcher pieces, then runs the component host
# against `aurora-demo1-ui`. Edit `crates/aurora-demo1-ui/src/demo1_view.rs` while it
# runs; the window rebuilds live with signals, springs, and tickers intact.
#
# Usage: ./demo1.sh  (from anywhere; paths anchor to this script's directory)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

# Vanilla builds only: the loop spawns the same command, and any flag difference
# (RUSTFLAGS, CARGO_INCREMENTAL) versions nothing but poisons dylib relinks.
# See docs/hot-reload.md.
#
# One package per invocation: multi-package dylib builds flake in split-sysroot
# envs ("std/core/alloc only shows up once" with zero source changes — observed,
# not theorized) while sequential single-package builds of the same tree land
# green. A real compile error fails its package every time and still surfaces.
build_all() {
    local failed=0
    for pkg in aurora-shared aurora-demo1-ui hot-host aurora-cli; do
        if ! cargo build -q -p "$pkg"; then
            echo "demo1.sh: build failed for $pkg." >&2
            failed=1
        fi
    done
    return "$failed"
}
# The dylib links flake intermittently in split-sysroot envs ("std/core/alloc
# only shows up once" with zero source changes — observed, not theorized). One
# retry pass flips it green; a real compile error fails every pass identically
# and still surfaces below.
if ! build_all; then
    echo "demo1.sh: retrying once…" >&2
    if ! build_all; then
        echo "demo1.sh: a real compile error is above — fix it and rerun." >&2
        echo "demo1.sh: if it is only the link flake, clean the hot units and retry:" >&2
        echo "demo1.sh:   cargo clean -p aurora -p aurora-shared -p hot-host -p aurora-hot-ui -p aurora-demo1-ui -p aurora-cli && ./demo1.sh" >&2
        exit 1
    fi
fi

exec ./target/debug/aurora hot --package aurora-demo1-ui --root "$ROOT" --title "Demo 1 Hot"
