# aurora-macros — Attribute macros for the Aurora UI engine

Proc-macro crate (must be its own crate by Rust rules).

## Provided macros

- `#[component]` — marks a function as a component that runs inside the hook
  scope, so `use_signal` and friends allocate per-instance state without
  plumbing. Expands to absolute `::aurora::` paths, which resolve both inside
  the `aurora` crate (via its `extern crate self as aurora` alias) and in
  downstream crates.
