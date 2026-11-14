#!/usr/bin/env bash
# Runs Aurora/Riode examples with self-hosted live hot-reloading.
#
# Usage:
#   ./scripts/example.sh <name> [options]
# Examples:
#   ./scripts/example.sh scroll_demo
#   ./scripts/example.sh scroll_demo --no-hot
#   ./scripts/example.sh scroll_demo --fps 144

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NAME="${1:-scroll_demo}"
shift || true

# Normalize dashes to underscores
NAME="${NAME//-/_}"

if [ ! -d "examples/$NAME" ]; then
    echo "example.sh: unknown example '$NAME' (expected directory examples/$NAME)." >&2
    exit 2
fi

# Set dynamic linking flags for code-swap hot reloading
export RUSTFLAGS="-C prefer-dynamic ${RUSTFLAGS:-}"

# Configure JSON structured telemetry sink
LOG_FILE="/tmp/riode-${NAME}-$(date +%Y%m%d-%H%M%S).jsonl"
export RIODE_LOG_FILE="$LOG_FILE"
echo "example.sh: tracing log -> $RIODE_LOG_FILE" >&2

# Determine if the example is a standalone package or defined under riode-examples
if [ -f "examples/$NAME/Cargo.toml" ]; then
    PKG_FLAG="-p $NAME"
    EX_FLAG=""
else
    PKG_FLAG="-p riode-examples"
    EX_FLAG="--example $NAME"
fi

# Run directly via cargo: the application self-hosts its own hot-reload engine
# shellcheck disable=SC2086
exec cargo run --release $PKG_FLAG $EX_FLAG -- "$@"
