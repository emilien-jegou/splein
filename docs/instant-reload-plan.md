# Instant reload plan (Linux-only, Dioxus-inspired, no ELF jump table v1)

Scope: Linux x86_64/aarch64 ELF only. No Windows/macOS/wasm paths. No jump table in v1.
Model: Dioxus dual system — RSX template IPC for literal edits + full cdylib rebuild for the rest.

## Background (verified)

- Dioxus `notes/architecture/07-HOTRELOAD.md`: two systems. RSX template reload
  (`HotReloadMsg { templates, assets, jump_table: Option }`) handles literal/dynamic
  values only; struct/expression/control-flow/`rsx!`-count changes fall through to
  rebuild; Subsecond jump table (`subsecond::call`, `APP_JUMP_TABLE`, `apply_patch`,
  `main`-anchored ASLR slide) handles Rust logic.
- Aurora today: `DevLoop` (watch → `cargo build -p` → `DylibReloader` versioned-copy
  dlopen → `activate_fresh_library` → full-root re-execute + reconcile). `RemoteNode`/
  `RemoteSnapshot` interpreter (`remote.rs`) is already the template-receiving half.
  `view!` macro (`macros/src/view.rs`) expands to builder chains with no template output.

## Tier 0 — bench (1 session, no behavior change)

- Add phase timing to `DevLoop`: cargo-resolve, tip codegen, link, dlopen+activate.
  Publish in `HotReloadStats`.
- Baseline: `nu scripts/toolbox.nu example text_smooth`, 3 saves, record wall times.
- Decision gate: confirms link+codegen dominates before building Tier 1.

## Tier 1 — template IPC for literal edits (the "instant" tier)

Goal: `view!` text/color/size/literal-prop edits apply in ms with zero compile.

**Gating (load-bearing): the switch is process role, never build profile.**
The hot loop builds at `--profile release`, so `debug_assertions` gating would
leave the tables dead exactly where needed. `DevLoop::new` arms a process-wide
live flag (`set_template_live(true)`); shipped apps never construct a dev loop
and keep the unarmed behavior (one predictable atomic load per literal, compiled
default returned, no lock, no allocation). No profile split, no per-frame cost
that matters in prod.

**Line-shift behavior:** slot keys are `{file}:{line}:{idx}`. Inserting lines
above a `view!` shifts the new source's lines while the registry holds old ones,
so the first such edit misses and rebuilds once, then re-registers and fast-path
resumes. A shape+order key would remove this; not worth it yet.

1. **Macro emits template.** `view!` keeps current builder expansion; adds
   `#[cfg(debug_assertions)]` const template: node tree + source location + slots for
   literals/`{expr}` splices. Pure data, `serde`-able.
2. **Conservative gate (Dioxus rule).** On file save: strip `view!` bodies, compare
   remainder old-vs-new. Identical → template path eligible. Any Rust change
   (handlers, hooks, branches, component count) → Tier 2 fallback. Never guess.
3. **Transport (Linux-local).** Unix-domain socket (or watcher+sidecar file v1):
   dev loop parses changed file with the macro's own parser, sends template delta
   to running host. Host applies via `RemoteHost::interpret`-style subtree rebuild,
   reusing keys so layout cache/focus survive. Record in stats (`template_reloads`).
4. **Fallback always available.** Template parse/apply failure → full Tier-2 rebuild,
   previous tree kept, error in stats. No new failure modes that kill the app.

Human validation: `text_smooth` literal edits (travel const, color, text) patch
without a build line in the log; structure/handler edits still trigger cargo build.

## Tier 2 — faster cdylib rebuild for everything else (no jump table)

Only if Tier-0 bench shows rebuild latency still hurts after Tier 1 absorbs the
common edits.

1. **Argv capture.** `RUSTC_WORKSPACE_WRAPPER`-style capture around
   `hot_build_command` (`dev_loop.rs:552-594`): record tip-crate `rustc` argv +
   link line.
2. **`rustc` replay.** On save, invoke recorded `rustc` directly (no cargo
   resolve/fingerprint), re-link `-shared`, keep `activate_fresh_library`
   (`component.rs:211-314`) untouched. Behind env flag.
3. **Optional: dev-codegen tip.** Generative UI crate at `opt-level=0` + debug
   symbols while host stays release. Human-gated on `text_smooth` motion parity;
   if visible, stay release and accept slower patches.

## Explicitly deferred

- ELF jump table (`object`-crate name-match, sentinel-anchored slide, `hot_call!`
  detours): only if Tier-2 logic-edit latency still misses subsecond after Tier 1.
- `syn`-interpreter for const/expr evaluation, cross-crate patching, non-Linux stubs.
- No snapshot/pixel-diff validation per AGENTS.md: human eyes close each tier on
  `text_smooth` (Y advances every `.`, no 7-then-1 jumps, no drift-axis split).

## Milestones & acceptance

- M0: bench numbers recorded.
- M1: literal edit → pixels change, no cargo spawn; logic edit → cargo build as today.
- M2 (if needed): same pixels as M1 for logic edits, lower wall time than baseline.
