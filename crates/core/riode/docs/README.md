# Aurora documentation

Developer guides. Each one is a topic, not a reference — for signatures, read the source or
`cargo doc`.

| Guide | What it covers |
|---|---|
| [Motion in Aurora](transitions.md) | Animating values and boxes, FLIP, reading geometry, debugging and testing motion. |
| [Retention in Aurora](retention.md) | Chunk replay, dirty-flag lifecycle, damage vs content, cache-key contracts. |

## The one-paragraph version

Aurora lays out and paints. It does not animate anything on your behalf.

Every transition is a value you own, held in a signal, integrated over time. A view that should
grow is a signal you animate; anything inside that view that should move with it reads that same
signal. There is no engine-side "animate this property" API, because a value the engine
interpolates is a value the client cannot read — and when a container's box and its content
disagree about a number, the content is what visibly breaks.

```rust
let width = Transition::spring(Spring::snappy())
    .on(expanded.clone().map(|e| if e { 196.0 } else { 52.0 }));

group()
    .width(width.clone())                        // the box
    .children([text(width.map(|w| /* ... */))])   // and its label, from the same value
```

Start with [Motion in Aurora](transitions.md).

## Keeping these honest

`tests/docs_transitions_api.rs` exercises every API the guides name. If a variant is renamed or a
signature changes, the documentation stops compiling rather than quietly becoming fiction.