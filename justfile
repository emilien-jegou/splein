# Splein dev commands. Run `just --list` (or bare `just`) to see them.
#
# Shell completion (recipe names): just ships its own generators, e.g.
#   just --completions zsh  > ~/.zfunc/_just    # + fpath+=(~/.zfunc) in .zshrc
#   just --completions bash > ~/.local/share/bash-completion/completions/just
#   just --completions fish > ~/.config/fish/completions/just.fish

set shell := ["bash", "-uc"]

# List all recipes.
default:
    @just --list

# Build the project. Extra args pass through to cargo.
build *args="":
    cargo build {{ args }}

# Run cargo tests. Extra args pass through to cargo.
test *args="":
    cargo test {{ args }}

# Run bacon (watch mode).
watch:
    bacon

# Run an example in hot-reload mode (default: demo1). Engines: tiny-skia, vello, vello-cpu, wgpu.
example name="demo1" engine="":
    ./scripts/example.sh {{ name }} {{ engine }}

# Install dependencies in release folder (runs first for release tasks).
[private]
release-prepare:
    cd scripts/release && bun install

# Generate release commit and git tags.
relacher: release-prepare
    bun ./scripts/release/release.ts

# Generate release patch for github and deploy changes.
release-github: release-prepare
    bun ./scripts/release/release-gh.ts
