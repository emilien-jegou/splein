# Aurora hot-reload

State-preserving live rebuilds for UI developed against `aurora`. Edit a component,
save, and the running window rebuilds from the fresh library with signals, springs,
and tickers intact.

## The loop

Five stages, same for both flavors below:

1. **Detect** — `FileWatcher` notices source (or manifest) changes; polled per frame
   *and* per idle wake (`on_idle`), because an idle app presents no frames to poll from.
   A throttled freshness walk over the depinfo source closure backs it up, so saves the
   watcher missed (before startup, during restarts) still converge instead of serving
   stale pixels.
2. **Rebuild** — `DevLoop` runs `cargo build -p <ui>` in the background (one vanilla
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
  *single shared runtime image* (`aurora-shared`) dynamically and nothing else, so
   hooks resolve against the host's live graph. Entry points: `aurora_hot_version`,
   `aurora_component_root`. Driven by `ComponentReloadExtension`; hosted by `aurora`
   (`hot` command) + `hot-host`.
- **Remote loop** (data descriptions interpreted host-side): UI returns plain data,
  host owns every signal. No shared instance needed. Driven by
  `RemoteReloadExtension`; entry points: `aurora_remote_version`, `aurora_remote_state`,
  `aurora_remote_build`, `aurora_remote_event`.

## Running it

The component loop is hosted by the `aurora` launcher + `hot-host` (shared-instance
land — see Constraints). `--package` selects the UI crate, so the demo1 loop is the
same flow against `aurora-demo1-ui`:

```bash
# Terminal 1: build the launcher pieces once
cargo build -p aurora-cli -p hot-host -p aurora-shared -p aurora-hot-ui -p aurora-demo1-ui

# Terminal 1 (cont.): run the component host against the demo1 UI
./target/debug/aurora hot --package aurora-demo1-ui

# Terminal 2: edit crates/aurora-demo1-ui/src/demo1_view.rs (try the outer fill),
# save.
# Watch the log: `rebuilding …` → `components reloaded` → `rebuilt`. A failing build
# logs its stderr tail and the log path; the previous UI stays mounted.

# The original demo UI: same flow with `--package aurora-hot-ui`, editing
# crates/aurora-hot-ui/src/lib.rs (try the hero fill).
```

Headless verification without a window:

```bash
./target/debug/aurora selftest --ui target/debug/libaurora_hot_ui.so; echo $?
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
- **One dynamic edge.** Hot-loop crates link exactly one aurora-flavored image; extra
  rlib co-deps break linking (observed, not theorized). A driver that embeds its own
  aurora copy is refused at activation with a message (sentinel check), never a
  midpoint panic. Drive via `aurora hot` + `libaurora_shared.so`. DevLoop appends
  exactly one flag of its own — a `--sysroot` pin, skipped on split-toolchain
  installations where the pin itself duplicates sysroot crates; keep hand builds
  vanilla otherwise or fingerprints skew across invocations.
- **State converges; events accelerate.** The watcher is the fast path, but the loop
  also compares source freshness (cargo depinfo closure) against the loaded image —
  saves from before startup or during restarts still rebuild. Edits can no longer be
  silently lost.
- **Machine-checked handshake.** `aurora_hot_version` / `aurora_remote_version` return
  an ABI fingerprint (protocol generation + boundary-type layouts + toolchain id),
  not a hand-bumped integer. Signature drift and toolchain skew fail closed with a
  message.
- **No hooks-in-anonymous-voids.** Registrations made outside any build window are
  untracked and never swept (yesterday's behavior, unchanged).

## Testing map

| What | Where | How |
|---|---|---|
| Rebuild preserves state; panics keep old tree; impact stats | `crates/aurora/tests/hot_reload.rs` (10 tests) | `cargo test -p aurora --test hot_reload` |
| Dev cycle (watch→build→swap) + handshake | `hot_component_e2e.rs` (component), `hot_remote_e2e.rs` (remote) | headless, real cargo builds |
| Freshness convergence, depinfo parse, loop-state display | `hot_reload::dev_loop` unit tests | `cargo test -p aurora --lib hot_reload::` |
| Two-runtime refusal (sentinel mismatch from a static-linked test binary) | `hot_component_e2e.rs::foreign_image_binds_sentinel_elsewhere` | headless, real image |
| Full component execution incl. clicks | `hot_host_selftest` via `aurora selftest` | headless, in shared-instance land (a cargo test binary *cannot* do this — it links its own static aurora and panics `hook called outside a component body`, then SIGABRT on unwind) |
| Reactive-rebuild non-accumulation, abort hygiene, `name=` | `hot_reload.rs` (ticker/abort/named tests) | headless |
| Pixels on screen | Xvfb run of the stub + save + observe logs (screenshots don't capture uncomposited X11 reliably; scene contents are asserted headlessly instead) | manual |

API detail lives in rustdoc: `aurora::hot_reload::{HotReload, DevLoop, DylibReloader, ComponentReloadExtension, RemoteReloadExtension, RemoteHost}`.
