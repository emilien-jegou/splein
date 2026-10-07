# bench_suite

Criterion benchmark suite for Aurora core performance gates (ported from the
Martensite `bench_suite`; workspace member, host-only).

Run: `cargo bench -p bench_suite --bench bench_suite`
Single bench: `cargo bench -p bench_suite --bench bench_suite -- <filter> --measurement-time 2 --sample-size 10`
Strict exit gates: `AURORA_STRICT_BENCH=1` (off by default; local runs just report).

| Bench | What it measures |
|---|---|
| `signal_chain_1k` | Linear `Signal -> Derived` chain set+pull. Scaled 10k -> 1k: Aurora's lazy-pull `Derived::get` re-evaluates recursively, so 10k overflows the stack rather than measuring throughput. |
| `tree_arena_10k` | (Replaces Martensite `WidgetArena` bench.) Lifecycle/traversal/compaction on the real generational `TreeArena`. |
| `diamond_reactive_network` | 1k diamonds in `batch()`, with the glitch-free single-evaluation exit gate. |
| `layout_1k_taffy` | Two-pass layout + leaf-restyle recompute on a 1051-node tree, taffy-direct (Aurora has no `LayoutEngine` wrapper). |
| `signal_fan_out_200` | 1 root -> 200 derived, dashboard scale. |
| `text_shaping` | `labels_500_cached` (cache-hit path) vs `labels_500_cold` (shaper path) via `TextContext`. Do not conflate the two. |
| `reorder_and_scroll` | (Replaces `DataTable` bench.) 1k keyed-list rotate + `set_root` + frame, and spacer-driven scroll step over 500 rows through the real `Engine`. |

Skew note: the pure-reactive micros must be read next to per-phase
`FrameTimings` from real frames, never as frame-time evidence on their own.
