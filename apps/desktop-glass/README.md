# `desktop-glass` — hot-reloadable glass-island overlay

Fullscreen transparent `Layer::Overlay` presenting the three-island glass
demo (`REGULAR` / `CLEAR` / `LIQUID` glass cards + a plain control card)
over the desktop. Each glass card becomes an island subsurface with its
own blur region + `riode_item_v1` hint; the riode-glass Hyprland plugin
renders the effect behind them. Island extraction, subsurface pool, and
the hint protocol live in `riode-platform-wayland`.

## Hot-reload the view

The view is **not** in this folder — it lives at
`examples/desktop_glass/view.rs` (shared with the windowed preview
`cargo run -p riode-examples --example desktop_glass` and the hot entry
`examples/desktop_glass/hot.rs`). This overlay is its host: edit the view
while the overlay runs and the cards rebuild live, hooks and island
state preserved across the swap.

**Run it:**

```sh
./scripts/desktop-glass-hot.sh   # or: dev desktop-glass
```

The script builds the shared runtime + UI image with the unit-family
flags (`-C prefer-dynamic`), then runs this overlay, which:
1. preloads `target/release/libriode_shared.so` globally first, so the
   UI image resolves riode to the one live runtime;
2. boot-builds the UI image, installs its `riode_component_root` as the
   engine's root factory, and polls `DevLoop` every frame — a save in
   `examples/desktop_glass/` rebuilds and swaps the root automatically.

Requires a running Hyprland session (layer-shell overlay).

## Why the host links `riode-shared`, never static `riode`

The hot dylib (`riode-hot-gen-ui`) resolves its riode symbols to the
preloaded shared image. If this host linked its own static copy, the
sentinel handshake would refuse the UI (`statically-linked driver`) and
every cross-boundary `Element` would be a second, incompatible copy.
`apps/desktop-glass` therefore depends on `riode-shared` (dylib) only —
keep it that way.
