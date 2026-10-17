# `riode-glass` — Hyprland plugin (IPC + desktop-backdrop glass rendering)

See `research-hyprland-plugin.md` §5 for the phased plan. The plugin renders:
load/unload-safe with a version/hash guard, `hyprctl riode status`, the
item-helper IPC (ABI 7, wire v5 — exact-size packed frames), per-island
backdrop blur (`GlassBlur`) and the optics pass (`GlassOptics`: lens,
frost, glow, 16 blend modes — line-for-line with the wgpu twin
`riode-engine-wgpu/src/shaders/glass.wgsl`; change both together).

## Build

Needs Hyprland dev headers (`pkg-config hyprland` — provided by `pkgs.hyprland`
in `devenv.nix`, or by `hyprpm` headers on a HyprlandOS/NixOS machine):

```sh
cd plugins/riode-glass
make            # -> riode-glass.so
```

Or via hyprpm (handles header provisioning + pinning itself):

```sh
hyprpm add /home/emilien/projects/splein/plugins/riode-glass
hyprpm enable riode-glass
hyprpm reload
```

## Testing scenario (human-eyes gate, per AGENTS.md)

1. `hyprpm reload` (or `hyprctl plugin load ./riode-glass.so` from this dir).
2. `hyprctl plugin list` — `riode-glass` listed, description is NOT "Paused (...)".
3. `hyprctl riode status` — one line: "plugin blur active" (or
   "rendering disabled (hook not installed)").
4. `hyprctl -j riode status` — JSON with plugin/version/built_for/rendering;
   with a live app, `items_detail` shows `glass:` islands latched at
   glass_version 5.
5. `hyprctl plugin unload riode-glass` (or `hyprpm disable riode-glass && hyprpm reload`)
   — compositor survives, no crash; reload again — still clean (no duplicate-guard trip).
6. Negative control: `RIODE_GLASS_SKIP_VERSION_CHECK=1` is the only bypass; without
   it, a Hyprland update shows "Paused (hyprland_version)" instead of crashing.

## What's next (not this phase)

- renderLayer redirect ("3b") — the cure noted in `GlassBlur.hpp`; the
  current hook cannot hit every case.
- Node/layer blend and glass/shadow/DSL blend: `plan-blend-mode-full.md`.
- The shader twins (`glass.wgsl`, `GlassOptics.cpp`) must stay in sync —
  a stage retired on one side must be retired on the other.
