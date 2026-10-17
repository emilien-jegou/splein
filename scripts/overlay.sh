#!/usr/bin/env bash
# Standalone overlay-app hot-reload loop (for-development): builds the
# launcher pieces, then runs the app's own overlay host binary against its
# own cdylib UI (`[lib] crate-type = ["cdylib"]` in the app package).
# Edit the app's `src/` while it runs; the overlay rebuilds live.
#
# Usage: nu scripts/toolbox.nu apps <app>  (e.g. `dev apps dock`)
# Apps: dock (more standalone overlay apps add a case arm below).
#
# Contrast `example.sh --overlay`: that serves generative examples whose UI
# lives under `examples/<name>/` (mounted through `riode-hot-gen-ui`).
# Standalone apps keep their UI next to the app (`apps/<app>/src/`), so
# they hand-build their own package set and exec their own binary — never
# via `cargo run`, which would re-resolve the graph in a second invocation.
set -euox pipefail
# This script lives in scripts/: the workspace root is one level up.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APP="${1:-dock}"

case "$APP" in
    dock) HOST_BIN="dock"; HOST_SET="dock" ;;
    *)
        echo "overlay.sh: unknown standalone overlay app '$APP'." >&2
        echo "overlay.sh: usage: nu scripts/toolbox.nu apps <app>" >&2
        exit 2 ;;
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
    RUSTFLAGS="-C prefer-dynamic" \
        cargo build --release "${pkgs[@]}"
}

# Everything is `--release`: runtime speed matters for interactive demos, and a
# single profile everywhere keeps one unit family (the loop rebuilds release too).
hot_build_for_set "$ROOT/scripts/hot-sets/$HOST_SET"
export LD_LIBRARY_PATH="$ROOT/target/release${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

# Auto-created trace log: JSON tracing (incl. per-pass span timings) lands here
# so it can be handed to an agent for analysis. Honored by
# `riode::hot_reload::init_dev_logging` via `RIODE_LOG_FILE`.
LOG_FILE="/tmp/riode-${APP}-overlay-$(date +%Y%m%d-%H%M%S).jsonl"
export RIODE_LOG_FILE="$LOG_FILE"
echo "overlay.sh: tracing log -> $RIODE_LOG_FILE" >&2

exec ./target/release/"$HOST_BIN"
