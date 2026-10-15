# Aurora hot-reload

State-preserving live rebuilds for UI developed against `riode`. Edit a component,
save, and the running window rebuilds from the fresh library with signals, springs,
and tickers intact.

## The loop

Five stages, same for both flavors below:

1. **Detect** — `FileWatcher` notices source (or manifest) changes; polled per frame
   *and* per idle wake (`on_idle`), because an idle app presents no frames to poll from.
   A throttled freshness walk over the depinfo source closure backs it up, so saves the
   watcher missed (before startup, during restarts) still converge instead of serving
   stale pixels.
2. **Rebuild** — `DevLoop` runs one vanilla `cargo build` over the shared image
   plus the generative UI crate (`-p riode-shared -p riode-hot-gen-ui` with
   `RIODE_HOT_EXAMPLE=<name>`) in the background; the
   static launcher binary (`riode-cli`) builds separately in the main target.
   The host's boot path runs the same canonical build synchronously *before*
   creating the window (see `blocking_hot_build`: script, loop, and boot all
   share one command constructor, so the unit family can never diverge).
  The only flag the loop
  ever adds is a
   command per cycle so fingerprints agree; the only flag it ever adds is a
   `--sysroot` pin, skipped on split-toolchain installations where the pin itself
   duplicates sysroot crates — see `rustc_sysroot`). Saves landing mid-build re-queue
   instead of being dropped. The spawn logs its build-log path; failures log the
   stderr tail and record it in stats.
3. **Swap** — `DylibReloader` loads the fresh image on mtime change (torn mid-link
   files fail soft and retry). Superseded images stay mapped; a version handshake
   refuses protocol drift with a message instead of misbehaving.
4. **Re-execute** — the engine drain re-runs the root factory under the same stable
   `"root"` hook scope, inside a generation-tagged build window. Commit keeps the new
   registrations and drops superseded twins; abort drops only the failed build's.
   A panicking rebuild keeps the previous tree and stages the error for one HUD frame.
5. **Reconcile** — the normal reconciler diffs the new tree against the live arena:
   same keys reuse nodes, layout caches and focus survive; only changed subtrees
   re-layout and repaint. Impact (`laid_out`, `nodes_dirtied`, `damage_rects`) is
   recorded per reload in `HotReloadStats`.

Manual triggers: `F6`, the debug panel Reload button, `ReloadAll` / `ReloadComponent`
bus commands, or the `queue_*` API.

## Two flavors

- **Component loop** (real `#[component]` bodies re-executed): UI crate links the
  *single shared runtime image* (`riode-shared`) dynamically and nothing else, so
   hooks resolve against the host's live graph. Entry points: `riode_hot_version`,
   `riode_component_root`. Driven by `ComponentReloadExtension`; hosted by `riode`
   (`hot` command) + `riode-devkit-bin`.
- **Remote loop** (data descriptions interpreted host-side): UI returns plain data,
  host owns every signal. No shared instance needed. Driven by
  `RemoteReloadExtension`; entry points: `riode_remote_version`, `riode_remote_state`,
  `riode_remote_build`, `riode_remote_event`.

## Running it

The component loop is hosted by the `riode` launcher + `riode-devkit-bin` (shared-instance
land — see Constraints). `--example` selects the example (the generative UI
crate mounts its `view.rs`/`hot.rs`); `nu scripts/toolbox.nu example` wraps
the whole flow:

```bash
# Terminal 1: run the component host against an example (generative UI)
nu scripts/toolbox.nu example demo1

# Terminal 2: edit examples/demo1/view.rs (try the outer fill), save.
# Watch the log: `rebuilding …` → `components reloaded` → `rebuilt`. A failing build
# logs its stderr tail and the log path; the previous UI stays mounted.
```

### Every example, both modes

All demos live in top-level `examples/` as static release binaries:

```bash
cargo run -p riode-examples --example motion_press --release
```

Every tree-only view is additionally hot-reloadable through the generative UI
crate (`riode-hot-gen-ui`, single dynamic edge against `riode-shared`):
each `examples/<name>/main.rs` keeps only its App chrome, the tree lives in
`examples/<name>/view.rs` alongside `HOT_TITLE`/`HOT_WIDTH`/`HOT_HEIGHT`/`HOT_BG`
consts (single source — `main.rs` references them too), and
`examples/<name>/hot.rs` wires the modules plus a single
`define_hot_ui! { root, title, size, background }` block emitting all four
entry points (`riode_hot_version`, `riode_component_root`,
`riode_hot_window`, `riode_hot_title`). The host builds the first image
synchronously, reads the window symbols, and creates the window once at final
chrome — no wrong-sized frame ever shows, and the clock stays frozen until
reveal. Const edits live-resize through the rebuild path. Same loop, any example:

```bash
nu scripts/toolbox.nu example motion_press
# Terminal 2: edit examples/motion_press/view.rs, save → rebuilding → reloaded.
```

Hot-reloadable: demo1, demo2, demo3, motion_accordion, motion_playback,
motion_imperative, motion_interruption, motion_layout_split, motion_press,
motion_reorder, motion_signals, motion_transitions, motion_validate,
selectable_text, shadow_lab, shadow_studio, scroll_demo, text_demo,
text_smooth, view_demo, wave1_repro, wrap_repro.

Release-only (App-extension drivers the tree loop cannot host — extensions,
switcher menus, winit plumbing, headless engines): moving, showcase,
selection_showcase, studio, render_bench.
`nu scripts/toolbox.nu example <that-name>` refuses with the release command; it never half-boots.

Headless verification without a window:

```bash
./target/release/riode selftest --ui target/release/libriode_hot_ui.so; echo $?
```

## State model (the contract)

- Stable identity preserves state: `module_path!()::Name`, or `#[component(name = …)]`
  to survive file moves. Ordinal hooks need unchanged call order; keyed hooks
  (`use_signal!`) survive reordering and conditional skips.
- Reactive list rebuilds re-run row bodies; generation windows sweep superseded
  registrations so ticker counts track live rows, not rebuild history. (Known residual:
  *removed* rows never re-run a body, so nothing sweeps their timers — same as before
  this machinery; route switches still retire by ticker epoch.)
- Build failures keep the old UI; the cargo stderr tail lands in
  `HotReloadStats::{last_build_error, last_build_log}` (the loop waits for the next
  save rather than retrying blindly).

## Constraints (load-bearing, not stylistic)

- **Linux-first.** Library paths default to `target/debug/*.so`; same toolchain both
  sides (Rust has no stable ABI) — now machine-checked, see below.
- **One dynamic edge.** Hot-loop crates link exactly one riode-flavored image; extra
  rlib co-deps break linking (observed, not theorized). A driver that embeds its own
  riode copy is refused at activation with a message (sentinel check), never a
  midpoint panic. Drive via `riode hot` + `libriode_shared.so`. DevLoop appends
  exactly one flag of its own — a `--sysroot` pin, skipped on split-toolchain
  installations where the pin itself duplicates sysroot crates; keep hand builds
  vanilla otherwise or fingerprints skew across invocations.
- **One cargo process, one canonical root set.** `scripts/example.sh` and every
   loop-spawned rebuild run `cargo build` over the shared image plus the
   generative UI crate (`-p riode-shared -p riode-hot-gen-ui` with
   `RIODE_HOT_EXAMPLE=<name>` set) — the example varies per run via env,
   shared is the constant companion — so the shared image
  and its consumer resolve within a single unit graph. (The static
  `riode-cli` launcher builds separately in the main target; it links
  nothing riode-flavored so it can't disturb the dylib graph.) DevLoop
  appends exactly one flag of its own — a `--sysroot` pin, skipped on
  split-toolchain installations where the pin itself duplicates sysroot
  crates; keep hand builds vanilla otherwise or fingerprints skew across
  invocations. Never run two builders concurrently on one `target/` dir, and
  don't save sources mid-build: overlapping builds interleave fingerprints
  and outputs behind cargo's back.
- **State converges; events accelerate.** The watcher is the fast path, but the loop
  also compares source freshness (cargo depinfo closure) against the loaded image —
  saves from before startup or during restarts still rebuild. Edits can no longer be
  silently lost.
- **Machine-checked handshake.** `riode_hot_version` / `riode_remote_version` return
  an ABI fingerprint (protocol generation + boundary-type layouts + toolchain id),
  not a hand-bumped integer. Signature drift and toolchain skew fail closed with a
  message.
- **No hooks-in-anonymous-voids.** Registrations made outside any build window are
  untracked and never swept (yesterday's behavior, unchanged).

## Testing map

| What | Where | How |
|---|---|---|
| Rebuild preserves state; panics keep old tree; impact stats | `crates/riode/tests/hot_reload.rs` (10 tests) | `cargo test -p riode --test hot_reload` |
| Dev cycle (watch→build→swap) + handshake | `hot_component_e2e.rs` (component), `hot_remote_e2e.rs` (remote) | headless, real cargo builds |
| Freshness convergence, depinfo parse, loop-state display | `hot_reload::dev_loop` unit tests | `cargo test -p riode --lib hot_reload::` |
| Two-runtime refusal (sentinel mismatch from a static-linked test binary) | `hot_component_e2e.rs::foreign_image_binds_sentinel_elsewhere` | headless, real image |
| Full component execution incl. clicks | `devkit_selftest` via `riode selftest` | headless, in shared-instance land (a cargo test binary *cannot* do this — it links its own static riode and panics `hook called outside a component body`, then SIGABRT on unwind) |
| Reactive-rebuild non-accumulation, abort hygiene, `name=` | `hot_reload.rs` (ticker/abort/named tests) | headless |
| Pixels on screen | Xvfb run of the stub + save + observe logs (screenshots don't capture uncomposited X11 reliably; scene contents are asserted headlessly instead) | manual |

API detail lives in rustdoc: `riode::hot_reload::{HotReload, DevLoop, DylibReloader, ComponentReloadExtension, RemoteReloadExtension, RemoteHost}`.
