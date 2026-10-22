#!/usr/bin/env bash
# Generic example hot-reload loop (for-development): builds the launcher pieces,
# then runs the component host against the generative UI crate built for the
# example (`riode-hot-gen-ui` with `AURORA_HOT_EXAMPLE=<name>`).
# Edit `examples/<name>/view.rs` while it runs; the window rebuilds live.
#
# Usage: nu scripts/toolbox.nu example <name> [engine]  (e.g. nu scripts/toolbox.nu example motion_press wgpu; default: demo1)
# Engines: tiny-skia (default), wgpu
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
OVERLAY=""

# Flags may arrive in any position: `example <name> [engine] [--overlay]`.
for arg in "$@"; do
    case "$arg" in
        --overlay) OVERLAY=1 ;;
    esac
done
if [ "$ENGINE" = "--overlay" ]; then
    ENGINE=""
fi

# Dashes are user spelling; dirs use underscores (`desktop-glass` -> `desktop_glass`).
NAME="${NAME//-/_}"

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

# Hot-reload package sets live in scripts/hot-sets/ (one file per host kind):
# the same file the Rust loop reads via riode::hot_reload::companion_set.
# Both sides always pass cargo the identical `-p` set, so one unit family
# resolves everywhere. Never inline `-p` lists here — read the files.
hot_build_for_set() {
    local set_file="$1"
    local -a pkgs=()
    local tok
    # Word-split the set file, stripping `#` comments (whole-line and
    # trailing) and blank lines. Comment text must never reach cargo:
    # a stray flag word in a comment would be passed as an argument.
    for tok in $(sed 's/#.*//' "$set_file"); do
        pkgs+=(-p "$tok")
    done
    RIODE_HOT_EXAMPLE="$NAME" RUSTFLAGS="-C prefer-dynamic" \
        cargo build --release "${pkgs[@]}"
}

# Build order (documented — do not reorder, do not merge):
#   1. `riode-cli` alone: static launcher binary. Built separately because
#      its clap-derive proc-macro closure spawns rival unit variants that break
#      the dylib links below when co-built (proven by same-hour with/without
#      trials). Static links are immune, so it builds in isolation.
#   2. Grouped set from scripts/hot-sets/<host>: one invocation, one unit
#      family. The loop rebuilds the same set (see DevLoopConfig), and
#      `RIODE_HOT_EXAMPLE` selects which example the generative crate mounts.
# Everything is `--release`: runtime speed matters for interactive demos, and a
# single profile everywhere keeps one unit family (the loop spawns release too).
# Overlay mode: the transparent layer-shell host (not a winit window), so the
# compositor + riode-glass plugin effects are visible while tuning.
# Only examples whose view is overlay-shaped make sense here
# (desktop_glass, prism_only, wire_grain); anything else exits loud
# instead of booting a mis-sized overlay. Standalone overlay apps (dock)
# run via `scripts/overlay.sh` instead — their UI lives next to the app,
# not under `examples/`.
if [ -n "$OVERLAY" ]; then
    case "$NAME" in
        desktop_glass|prism_only|wire_grain) HOST_BIN="desktop-glass"; HOST_SET="desktop-glass" ;;
        *)
            echo "example.sh: --overlay supports desktop_glass, prism_only, wire_grain (got '$NAME')." >&2
            exit 2 ;;
    esac
    if [ -n "$ENGINE" ]; then
        # The overlay runner rasterizes with headless tiny-skia; glass is
        # rendered by the riode-glass compositor plugin from the packed
        # frame, so a raster-engine choice has no effect here.
        echo "example.sh: overlay raster is headless tiny-skia (glass comes from the riode-glass plugin); engine '$ENGINE' ignored." >&2
    fi
    # The host binary is part of the set, so exec it directly — never via
    # `cargo run`, which would re-resolve the graph in a second invocation.
    hot_build_for_set "$ROOT/scripts/hot-sets/$HOST_SET"
    export LD_LIBRARY_PATH="$ROOT/target/release${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    # The host reads this: boot chrome (HOT_*), the loop's rebuild example,
    # and the dylib mount must all name the same example.
    export RIODE_HOT_EXAMPLE="$NAME"
    LOG_FILE="/tmp/riode-${NAME}-overlay-$(date +%Y%m%d-%H%M%S).jsonl"
    export RIODE_LOG_FILE="$LOG_FILE"
    echo "example.sh: tracing log -> $RIODE_LOG_FILE" >&2
    exec ./target/release/"$HOST_BIN"
fi
cargo build --release -p riode-cli
hot_build_for_set "$ROOT/scripts/hot-sets/window"

if [ -n "$ENGINE" ]; then
    ENGINE_FLAG="--engine $ENGINE"
else
    ENGINE_FLAG=""
fi

# Auto-created trace log: JSON tracing (incl. per-pass span timings) lands here
# so it can be handed to an agent for analysis. Honored by
# `riode::hot_reload::init_dev_logging` via `RIODE_LOG_FILE`.
LOG_FILE="/tmp/riode-${NAME}-$(date +%Y%m%d-%H%M%S).jsonl"
export RIODE_LOG_FILE="$LOG_FILE"
echo "example.sh: tracing log -> $RIODE_LOG_FILE" >&2

# shellcheck disable=SC2086
exec ./target/release/riode hot --example "$NAME" --root "$ROOT" \
    $ENGINE_FLAG
