#!/usr/bin/env bash
# Hot-reloadable desktop-glass overlay (dev only). Thin wrapper kept for
# muscle memory: the canonical path is `dev example desktop-glass --overlay`
# (scripts/example.sh), which owns the hand-build package set alongside the
# Rust loop (see scripts/hot-sets/). Do NOT add a second build shape here —
# any deviation from the canonical set compiles rival units and breaks loads.
#
# Usage: ./scripts/desktop-glass-hot.sh
# Requires a running Hyprland session (layer-shell overlay).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$ROOT/scripts/example.sh" desktop-glass --overlay
