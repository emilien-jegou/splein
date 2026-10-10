# Aurora hot-reload

State-preserving live rebuilds for UI developed against `aurora`. Edit a component,
save, and the running window rebuilds from the fresh library with signals, springs,
and tickers intact.

## The loop

Five stages, same for both flavors below:

1. **Detect** — `FileWatcher` notices source (or manifest) changes; polled per frame
   *and* per idle wake (`on_idle`), because an idle app presents no frames to poll from.
2. **Rebuild** — `DevLoop` runs `cargo build -p <ui>` in the background (one vanilla
   command per cycle so fingerprints agree; never with custom `RUSTFLAGS`). Saves
   landing mid-build re-queue instead of being dropped.
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
  `aurora_component_root`. Driven by `ComponentReloadExtension`; hosted by `hot-stub`
  + `hot-host`.
- **Remote loop** (data descriptions interpreted host-side): UI returns plain data,
  host owns every signal. No shared instance needed. Driven by
  `RemoteReloadExtension`; entry points: `aurora_remote_version`, `aurora_remote_state`,
  `aurora_remote_build`, `aurora_remote_event`.

## Running it

```bash
# Terminal 1: build the launcher pieces once
cargo build -p hot-stub -p hot-host -p aurora-shared -p aurora-hot-ui

# Terminal 1 (cont.): run the component host against the demo UI
./target/debug/hot-stub \
  --shared $PWD/target/debug/libaurora_shared.so \
  --host   $PWD/target/debug/libhot_host.so \
  --package aurora-hot-ui --root $PWD --title "Hot" --background 0xFFFFFF

# Terminal 2: edit crates/aurora-hot-ui/src/lib.rs (try the hero fill), save.
# Watch the log: `rebuilding … (sources changed)` → `components reloaded` → `rebuilt`.
```

Headless verification without a window:

```bash
./target/debug/hot-stub --shared …/libaurora_shared.so --host …/libhot_host.so \
  --selftest …/libaurora_hot_ui.so; echo $?
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
  sides (Rust has no stable ABI).
- **One dynamic edge.** Hot-loop crates link exactly one aurora-flavored image; extra
  rlib co-deps break linking (observed, not theorized). DevLoop never sets RUSTFLAGS;
  keep hand builds vanilla or fingerprints skew across invocations.
- **No hooks-in-anonymous-voids.** Registrations made outside any build window are
  untracked and never swept (yesterday's behavior, unchanged).

## Testing map

| What | Where | How |
|---|---|---|
| Rebuild preserves state; panics keep old tree; impact stats | `crates/aurora/tests/hot_reload.rs` (10 tests) | `cargo test -p aurora --test hot_reload` |
| Dev cycle (watch→build→swap) + handshake | `hot_component_e2e.rs` (component), `hot_remote_e2e.rs` (remote) | headless, real cargo builds |
| Full component execution incl. clicks | `hot_host_selftest` via `hot-stub --selftest` | headless, in shared-instance land (a cargo test binary *cannot* do this — it links its own static aurora) |
| Reactive-rebuild non-accumulation, abort hygiene, `name=` | `hot_reload.rs` (ticker/abort/named tests) | headless |
| Pixels on screen | Xvfb run of the stub + save + observe logs (screenshots don't capture uncomposited X11 reliably; scene contents are asserted headlessly instead) | manual |

API detail lives in rustdoc: `aurora::hot_reload::{HotReload, DevLoop, DylibReloader, ComponentReloadExtension, RemoteReloadExtension, RemoteHost}`.
