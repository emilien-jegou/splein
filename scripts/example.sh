#!/usr/bin/env bash
# Generic example hot-reload loop (for-development): builds the launcher pieces,
# then runs the component host against the generative UI crate built for the
# example (`riode-hot-gen-ui` with `AURORA_HOT_EXAMPLE=<name>`).
# Edit `examples/<name>/view.rs` while it runs; the window rebuilds live.
#
# Usage: nu scripts/toolbox.nu example <name> [engine]  (e.g. nu scripts/toolbox.nu example motion_press wgpu; default: demo1)
# Engines: tiny-skia (default), vello, vello-cpu, wgpu
# Release (no hot-reload, statically linked):
#   cargo run -p riode-examples --example <name> --release
# Hot-reloadable names (tree-only views: any examples/<name>/ dir with hot.rs):
#   border_lab gradient_lab image_lab demo1 demo2 demo3 motion_accordion motion_playback
#   motion_imperative motion_interruption motion_layout_split motion_press
#   motion_reorder motion_signals motion_transitions motion_validate
#   selectable_text shadow_lab shadow_studio scroll_demo text_demo text_smooth view_demo wave1_repro wrap_repro
# Release-only (App-extension drivers the tree loop cannot host — extensions,
# switcher menus, winit plumbing, headless engines):
#   moving showcase selection_showcase studio render_bench
set -euox pipefail
# This script lives in scripts/: the workspace root is one level up.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NAME="${1:-demo1}"
ENGINE="${2:-}"

case "$NAME" in
    moving|showcase|selection_showcase|studio|render_bench)
        echo "example.sh: '$NAME' is release-only (App-extension driver, not a hot-reloadable tree)." >&2
        echo "example.sh: run: cargo run -p riode-examples --example $NAME" >&2
        exit 2 ;;
    */*|*.*)
        echo "example.sh: invalid example name '$NAME'." >&2
        exit 2 ;;
    *)
        if [ ! -d "examples/$NAME" ]; then
            echo "example.sh: unknown example '$NAME'." >&2
            echo "example.sh: usage: nu scripts/toolbox.nu example <name>; release: cargo run -p riode-examples --example <name>" >&2
            exit 2
        fi ;;
esac

# Build order (documented — do not reorder, do not merge):
#   1. `riode-cli` alone: static launcher binary. Built separately because
#      its clap-derive proc-macro closure spawns rival unit variants that break
#      the dylib links below when co-built (proven by same-hour with/without
#      trials). Static links are immune, so it builds in isolation.
#   2. Grouped `riode-shared` + generative `riode-hot-gen-ui` +
#      shared image now owns host entry points.
#      graph. `RIODE_HOT_EXAMPLE` selects which example the generative crate
#      mounts; the loop itself rebuilds shared + UI with the same shape (see
#      DevLoopConfig::companion_packages).
# Everything is `--release`: runtime speed matters for interactive demos, and a
# single profile everywhere keeps one unit family (the loop spawns release too).
cargo build --release -p riode-cli
RIODE_HOT_EXAMPLE="$NAME" RUSTFLAGS="-C prefer-dynamic" cargo build --release -p riode-shared -p riode-hot-gen-ui

if [ -n "$ENGINE" ]; then
    exec ./target/release/riode hot --example "$NAME" --root "$ROOT" \
        --engine "$ENGINE"
else
    exec ./target/release/riode hot --example "$NAME" --root "$ROOT"
fi
