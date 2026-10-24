# `riode-glass` — Hyprland plugin (IPC + desktop-backdrop glass rendering)

See `research-hyprland-plugin.md` §5 for the phased plan. The plugin renders:
load/unload-safe with a version/hash guard, `hyprctl riode status`, the
item-helper IPC (ABI 8, wire v5 — exact-size packed frames), per-island
backdrop blur (`GlassBlur`) and the optics pass (`GlassOptics`: lens,
frost, glow, 16 blend modes — line-for-line with the wgpu twin
`crates/core/riode/src/foundation/effect/shaders/glass/` (one WGSL file
per effect, concatenated by `GLASS_COMBINED_WGSL` in
`effect/shaders.rs`; change both together).
Program-supplied custom effect stages ride the same helper
(`set_effect`/`clear_effects`/`set_glass_stage`): the module is
transpiled by the embedded naga glue blob (`src/shader-glue/`, see
`ShaderGlue.cpp`) and run as GL stages around the lens — no fallback,
a failed stage is skipped with a once-per-hash log.

## Build

Needs Hyprland dev headers (`pkg-config hyprland` — provided by `pkgs.hyprland`
in `devenv.nix`, or by `hyprpm` headers on a HyprlandOS/NixOS machine)
**and a Rust toolchain** (`make` builds the shader-glue cdylib with
cargo before embedding it):

```sh
cd plugins/riode-glass
make            # -> riode-glass.so (runs cargo for the glue blob)
```

### Embedded blobs: never reuse a `/proc/self/fd/N` path

Both embedded blobs (the item helper, the shader glue) are written to a
memfd and `dlopen`ed. **glibc's `dlopen` de-duplicates by path string**,
so if the loader closes the memfd, the fd number is recycled and the
next `dlopen("/proc/self/fd/3")` silently returns the **previous**
object — the shader glue ended up with the item helper's `.so`, and the
API symbol was "not found" (`undefined symbol:
riode_shader_glue_v1_api`). Fix (both loaders): unique memfd name per
load (`<kind>-<pid>-<seq>`) and the fd is **never closed**, so the path
string stays unique for the life of the object.

### Do not use `std::format` in this plugin

Hyprland links libstdc++ dynamically and **exports its weak
`std::__format` symbols** (93 of them). A plugin built against another
libstdc++ (this host: GCC 15 vs the compositor's GCC 16) binds its
`std::format` calls to the host's copies — mixed-version
`basic_format_arg`/`format_args` layout — and the **compositor segfaults
while loading the plugin** (see `src/Format.hpp` and the postmortem in
`ongoing-effect-work.md`, phase E5c). `std::string`/`std::vector` are
fine (C++11 ABI); `std::format` is C++20 and carries no ABI guarantee.

Format through `rfmt::format` (`src/Format.hpp`) instead: snprintf
backed, supports `{}`, `{:.Nf}`, `{:016x}`, `{{`/`}}`. Guard the tree
with:

```sh
nm -D riode-glass.so | grep __format   # must print nothing
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
   glass_version 5, effect-carrying islands as `+fx{N}@{glass_index}`
   (N stages latched, at the Glass slot; `@-1` = no glass). With the
   demo up, `glue_init` is `fresh` (first run) or `adopted`, and
   `stage_programs` counts compiled custom programs (`stage_failures`
   must stay 0 — a nonzero one is logged with its module hash).
5. `hyprctl plugin unload riode-glass` (or `hyprpm disable riode-glass && hyprpm reload`)
   — compositor survives, no crash; reload again — still clean (no duplicate-guard trip).
6. Negative control: `RIODE_GLASS_SKIP_VERSION_CHECK=1` is the only bypass; without
   it, a Hyprland update shows "Paused (hyprland_version)" instead of crashing.

## What's next (not this phase)

- renderLayer redirect ("3b") — the cure noted in `GlassBlur.hpp`; the
  current hook cannot hit every case.
- Node/layer blend and glass/shadow/DSL blend: `plan-blend-mode-full.md`.
- The shader twins (`shaders/glass/`, `GlassOptics.cpp`) must stay in
  sync — a stage retired on one side must be retired on the other.
