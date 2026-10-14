# riode-macros — Attribute macros for the Aurora UI engine

Proc-macro crate (must be its own crate by Rust rules).

## Provided macros

- `#[component]` — marks a function as a component that runs inside the hook
  scope, so `use_signal` and friends allocate per-instance state without
  plumbing. Expands to absolute `::riode::` paths, which resolve both inside
  the `riode` crate (via its `extern crate self as riode` alias) and in
  downstream crates. Always generates `{Name}Props` + builder — even for
  argless components, so `<Tag />` works uniformly.
- `view!` — RSX over the builder DSL (see `src/view.rs` header). Primitives
  (`Group`/`Text`/`Image`/`Svg`), `#[component]` tags via `{Tag}Props`,
  `{expr}` splices, and lowercase control-flow nodes (`<for>`, `<if>`,
  `<match>`).

## Conventions & gotchas

- **Integer literals need suffixes.** Prop setters take `impl Into<T>`, and
  Rust won't infer through them: `index={0}` fails (`i32` vs `usize`),
  `weight={700}` fails (vs `u16`). Write `index={0usize}`,
  `weight={700u16}`. The error names the expected type and points at the
  literal — suffix it and move on. Floats (`width={10.0}`) infer fine.
- **`children: Vec<Element>` vs `Vec<ChildSegment>`** — components take
  `Vec<Element>` and only ever receive static children; control nodes as
  direct component children are a compile error (hoist the node above the
  component). `Vec<ChildSegment>` is the engine's mixed static/dynamic child
  list, built by `view!` for `<Group>` children and control bodies.
- **`<for>` is rows, not a node.** It splices rows directly into the parent —
  there is deliberately no carrier node, no container styling on `<for>`
  itself (style the parent `<Group>`; nest one if the design needs it), and no
  second key: `key=` names item identity, full stop. Omitting `key=` falls
  back to positional identity with a warning. As a `view!` root, or inside
  `{…}`/attributes, a control node is an error with a pointer.
- **Borrowed values can't cross builders.** `<match>`/`<if>` arms and `<for>`
  row closures run in `children_builder`/`children_of` (`'static`), so
  scrutinees over `&` data must be cloned or hoisted — plain `if let` in a
  `{…}` splice stays inline and keeps borrows.
