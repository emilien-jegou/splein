# dock

Native macOS-style dock on the riode engine — a Rust port of the `backp`
web dock (`src/components/Dock.jsx` + `src/constants/apps.js` + dock CSS).

The bar is a compositor-level liquid-glass island: the engine paints glass
content-only over transparent and the riode-glass compositor plugin reads
the raw `Glass` material bytes (`set_glass`) to refract the live desktop
behind it. No preset names anywhere.

Run (dev, hot-reload): `dev apps dock` — edits to `src/view.rs`
rebuild and swap live without restarting.

* `src/lib.rs` — hot UI cdylib wiring (`define_hot_ui!`).
* `src/view.rs` — `Page` component + `HOT_*` window chrome (single source of truth).
* `src/registry.rs` — app table + dock order (ports `apps.js`).
* `src/dock.rs` — geometry constants + magnification law (ports `Dock.jsx` math).
* `src/icons.rs` — PNG/WebP decode into `ImageSource` (premultiplied RGBA).
* `src/main.rs` — overlay entrypoint: engine-level `overlay_run` config only.
* `assets/icons/` — copied verbatim from `backp/public/images/`.
