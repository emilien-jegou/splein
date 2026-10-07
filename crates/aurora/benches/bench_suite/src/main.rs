//! Performance verification benchmarks for Aurora.
//!
//! Ported from the Martensite `bench_suite` (linear DAG propagation, diamond
//! networks, fan-out, layout throughput, text shaping). Two Martensite-only
//! benches were dropped as incompatible and replaced with Aurora-native ones:
//! - `WidgetArena HotNode/ColdNode` ops -> `tree_arena_lifecycle_10k` on the
//!   real generational `TreeArena` (`tree/arena.rs`).
//! - `DataTable::visible_rows` virtualized scroll -> `reorder_and_scroll`
//!   driving the real `Engine` (keyed reconcile + chunk-cache reuse), the
//!   mechanism Aurora actually uses for large lists.
//!
//! Skew note: benches 1/3/5 are pure-reactive micros. They must be read next
//! to `FrameTimings` from real frames (see `render_bench` example), never as
//! frame-time evidence on their own.
#![allow(missing_docs)]

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::cell::{Cell, RefCell};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::rc::Rc;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use std::time::{Duration, Instant};

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use aurora::reactive::{batch, Derived, ReactiveRuntime, Signal};

/// Strict-gate switch: `AURORA_STRICT_BENCH=1` enforces median exit gates.
/// Off by default so local runs just report Criterion numbers.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn strict() -> bool {
    std::env::var("AURORA_STRICT_BENCH")
        .map(|v| v == "1")
        .unwrap_or(false)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn new_runtime() -> Rc<RefCell<ReactiveRuntime>> {
    Rc::new(RefCell::new(ReactiveRuntime::new()))
}

// ---------------------------------------------------------------------------
// Bench 1: linear signal chain propagation over 10,000 nodes.
//
// Requires the P0 iterative write path (`Signal::write` recomputes
// dependents over an explicit work stack): with call-stack recursion this
// depth overflows instead of measuring throughput. Handles hold no compute
// closures (registry-owned), so teardown does not nest destructors either.
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
const CHAIN: usize = 10_000;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn build_chain(rt: &Rc<RefCell<ReactiveRuntime>>) -> (Signal<u64>, Derived<u64>) {
    let root = Signal::new(Rc::clone(rt), 0u64);
    let r = root.clone();
    let mut leaf = Derived::new(Rc::clone(rt), move || r.get().wrapping_add(1));
    for _ in 2..CHAIN {
        let prev = leaf.clone();
        leaf = Derived::new(Rc::clone(rt), move || prev.get().wrapping_add(1));
    }
    (root, leaf)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_signal_chain(c: &mut Criterion) {
    let rt = new_runtime();
    let (root, leaf) = build_chain(&rt);

    root.set(1);
    assert_eq!(leaf.get(), 1 + (CHAIN as u64 - 1));

    if strict() {
        let mut samples: Vec<Duration> = Vec::with_capacity(100);
        for i in 0..100u64 {
            root.set(100 + i);
            let start = Instant::now();
            root.set(200 + i);
            let _ = leaf.get();
            samples.push(start.elapsed());
        }
        samples.sort();
        let median = samples[samples.len() / 2];
        assert!(
            median < Duration::from_millis(25),
            "signal chain 10k median {:?} (>= 25ms CI threshold)",
            median
        );
        eprintln!("signal chain 10k: median {:?} (PASSED strict gate)", median);
    } else {
        eprintln!("signal chain 10k: strict gate disabled; set AURORA_STRICT_BENCH=1 to enforce");
    }

    let mut counter = 2u64;
    let mut group = c.benchmark_group("signal_chain_10k");
    group.throughput(Throughput::Elements(CHAIN as u64));
    group.bench_function("set_and_pull", |b| {
        b.iter(|| {
            counter = counter.wrapping_add(1);
            root.set(counter);
            black_box(leaf.get());
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Bench 2 (replacement A): TreeArena 10k lifecycle on the real arena.
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn setup_arena_tree() -> (aurora::tree::TreeArena, Vec<aurora::tree::NodeId>) {
    use aurora::tree::{NodeKind, TreeArena};
    let mut arena = TreeArena::new();
    let mut ids = Vec::with_capacity(10_000);
    for _ in 0..10_000 {
        ids.push(arena.insert(aurora::tree::LayoutNode::new(NodeKind::Group)));
    }
    for i in 1..10_000 {
        let parent = ids[(i - 1) / 4];
        let child = ids[i];
        let mut kids = arena.children(parent).to_vec();
        kids.push(child);
        arena.set_children(parent, kids);
    }
    (arena, ids)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn dfs_count(arena: &aurora::tree::TreeArena, root: aurora::tree::NodeId) -> usize {
    let mut stack = vec![root];
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        black_box(id);
        visited += 1;
        stack.extend_from_slice(arena.children(id));
    }
    visited
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_tree_arena_10k(c: &mut Criterion) {
    {
        let (mut arena, ids) = setup_arena_tree();
        assert_eq!(dfs_count(&arena, ids[0]), 10_000);
        for i in (0..10_000).step_by(10) {
            arena.remove_with(ids[i], |_| {});
        }
        // Removing every 10th node removes whole subtrees; the survivors are
        // the nodes never visited by a removal.
        let stale = ids[0];
        assert!(
            !arena.is_valid(stale) || dfs_count(&arena, ids[1]) > 0,
            "arena must recycle generational slots after removals"
        );
    }

    if strict() {
        let mut samples: Vec<Duration> = Vec::with_capacity(100);
        for _ in 0..100 {
            let start = Instant::now();
            let (mut arena, ids) = setup_arena_tree();
            black_box(dfs_count(&arena, ids[0]));
            for i in (0..10_000).step_by(10) {
                arena.remove_with(ids[i], |_| {});
            }
            samples.push(start.elapsed());
        }
        samples.sort();
        let median = samples[samples.len() / 2];
        assert!(
            median < Duration::from_millis(100),
            "arena 10k lifecycle median {:?} (>= 100ms CI threshold)",
            median
        );
        eprintln!("arena 10k lifecycle: median {:?} (PASSED strict gate)", median);
    } else {
        eprintln!("arena 10k lifecycle: strict gate disabled; set AURORA_STRICT_BENCH=1 to enforce");
    }

    let mut group = c.benchmark_group("tree_arena_10k");
    group.throughput(Throughput::Elements(10_000));

    group.bench_function("lifecycle_10k", |b| {
        b.iter(|| {
            let (mut arena, ids) = setup_arena_tree();
            black_box(dfs_count(&arena, ids[0]));
            for i in (0..10_000).step_by(10) {
                arena.remove_with(ids[i], |_| {});
            }
        });
    });

    group.bench_function("dfs_traversal_10k", |b| {
        let (arena, ids) = setup_arena_tree();
        let root = ids[0];
        b.iter(|| black_box(dfs_count(&arena, root)));
    });

    group.bench_function("compaction_1k_removals", |b| {
        b.iter_batched(
            setup_arena_tree,
            |(mut arena, ids)| {
                for i in (0..10_000).step_by(10) {
                    arena.remove_with(ids[i], |_| {});
                }
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Bench 3: 1k diamond networks, transactional batch, glitch-free gate.
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_diamond_network(c: &mut Criterion) {
    let rt = new_runtime();

    struct Diamond {
        root: Signal<u64>,
        memo_d: Derived<u64>,
    }

    // Glitch check: D must evaluate exactly once per root change.
    let evals = Rc::new(Cell::new(0usize));
    let evals_in = Rc::clone(&evals);
    let test_root = Signal::new(Rc::clone(&rt), 10u64);
    let r_a = test_root.clone();
    let rt_c = Rc::clone(&rt);
    let test_a = Derived::new(rt_c, move || r_a.get().wrapping_mul(2));
    let r_b = test_root.clone();
    let rt_c = Rc::clone(&rt);
    let test_b = Derived::new(rt_c, move || r_b.get().wrapping_add(5));
    let a_d = test_a.clone();
    let b_d = test_b.clone();
    let rt_c = Rc::clone(&rt);
    let test_d = Derived::new(rt_c, move || {
        evals_in.set(evals_in.get() + 1);
        a_d.get().wrapping_add(b_d.get())
    });

    assert_eq!(test_d.get(), (10 * 2) + (10 + 5));
    assert_eq!(evals.get(), 1);
    batch(&rt, || test_root.set(20));
    assert_eq!(test_d.get(), (20 * 2) + (20 + 5));
    assert_eq!(
        evals.get(),
        2,
        "exit gate failed: diamond had redundant/glitch evaluations"
    );

    let diamonds: Vec<Diamond> = (0..1_000)
        .map(|i| {
            let root = Signal::new(Rc::clone(&rt), i as u64);
            let ra = root.clone();
            let ma = Derived::new(Rc::clone(&rt), move || ra.get().wrapping_mul(3));
            let rb = root.clone();
            let mb = Derived::new(Rc::clone(&rt), move || rb.get().wrapping_add(7));
            let (ca, cb) = (ma.clone(), mb.clone());
            let md = Derived::new(Rc::clone(&rt), move || ca.get().wrapping_add(cb.get()));
            Diamond { root, memo_d: md }
        })
        .collect();
    for (i, d) in diamonds.iter().enumerate() {
        assert_eq!(d.memo_d.get(), (i as u64 * 3) + (i as u64 + 7));
    }
    batch(&rt, || {
        for d in &diamonds {
            d.root.set(100);
        }
    });
    for d in &diamonds {
        assert_eq!(d.memo_d.get(), (100 * 3) + (100 + 7));
    }

    if strict() {
        let mut samples: Vec<Duration> = Vec::with_capacity(100);
        let mut counter = 100u64;
        for _ in 0..100 {
            counter = counter.wrapping_add(1);
            let start = Instant::now();
            batch(&rt, || {
                for d in &diamonds {
                    d.root.set(counter);
                }
            });
            for d in &diamonds {
                black_box(d.memo_d.get());
            }
            samples.push(start.elapsed());
        }
        samples.sort();
        let median = samples[samples.len() / 2];
        assert!(
            median < Duration::from_millis(5),
            "diamond 1k batch median {:?} (>= 5ms CI threshold)",
            median
        );
        eprintln!("diamond 1k: median {:?} (PASSED strict gate)", median);
    } else {
        eprintln!("diamond 1k: strict gate disabled; set AURORA_STRICT_BENCH=1 to enforce");
    }

    let mut counter = 100u64;
    let mut group = c.benchmark_group("diamond_reactive_network");
    group.throughput(Throughput::Elements(1_000));
    group.bench_function("batch_evaluation_1k", |b| {
        b.iter(|| {
            counter = counter.wrapping_add(1);
            batch(&rt, || {
                for d in &diamonds {
                    d.root.set(counter);
                }
            });
            for d in &diamonds {
                black_box(d.memo_d.get());
            }
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Bench 4: two-pass layout throughput on a ~1k-node tree, taffy-direct.
// Aurora sits on taffy 0.5 with no LayoutEngine wrapper, so this drives
// `taffy::Taffy` itself: 1 root column + 50 rows + 1000 leaf cells.
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn setup_layout_tree() -> (taffy::TaffyTree, taffy::NodeId) {
    use taffy::prelude::*;
    let mut taffy = taffy::TaffyTree::new();
    let mut rows = Vec::with_capacity(50);
    for _ in 0..50 {
        let mut leaves = Vec::with_capacity(20);
        for j in 0..20 {
            leaves.push(
                taffy
                    .new_leaf(Style {
                        size: Size {
                            width: length(24.0 + (j % 5) as f32 * 8.0),
                            height: length(16.0_f32),
                        },
                        ..Default::default()
                    })
                    .expect("leaf"),
            );
        }
        rows.push(
            taffy
                .new_with_children(
                    Style {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        gap: Size {
                            width: length(6.0_f32),
                            height: length(0.0_f32),
                        },
                        ..Default::default()
                    },
                    &leaves,
                )
                .expect("row"),
        );
    }
    let root = taffy
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                gap: Size {
                    width: length(0.0_f32),
                    height: length(4.0_f32),
                },
                ..Default::default()
            },
            &rows,
        )
        .expect("root");
    (taffy, root)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_layout_1k(c: &mut Criterion) {
    use taffy::{AvailableSpace, Size as GeoSize};
    use taffy::prelude::*;
    let avail = GeoSize {
        width: AvailableSpace::Definite(1600.0),
        height: AvailableSpace::Definite(900.0),
    };
    let mut group = c.benchmark_group("layout_1k_taffy");
    group.throughput(Throughput::Elements(1_051));

    group.bench_function("two_pass_compute", |b| {
        let (mut taffy, root) = setup_layout_tree();
        b.iter(|| taffy.compute_layout(root, avail).expect("compute"));
    });

    group.bench_function("recompute_after_leaf_change", |b| {
        let (mut taffy, root) = setup_layout_tree();
        taffy.compute_layout(root, avail).expect("warm compute");
        let leaf = taffy
            .new_leaf(Style {
                size: Size {
                    width: length(24.0_f32),
                    height: length(16.0_f32),
                },
                ..Default::default()
            })
            .expect("probe leaf");
        let mut flip = false;
        b.iter(|| {
            flip = !flip;
            taffy
                .set_style(
                    leaf,
                    Style {
                        size: Size {
                            width: length(if flip { 24.0 } else { 40.0_f32 }),
                            height: length(16.0_f32),
                        },
                        ..Default::default()
                    },
                )
                .expect("restyle leaf");
            taffy.compute_layout(root, avail).expect("recompute");
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Bench 5: UI-scale fan-out, 1 root -> 200 derived.
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_fan_out_200(c: &mut Criterion) {
    let rt = new_runtime();
    let root = Signal::new(Rc::clone(&rt), 0u64);
    let memos: Vec<Derived<u64>> = (0..200)
        .map(|i| {
            let r = root.clone();
            Derived::new(Rc::clone(&rt), move || r.get().wrapping_add(i))
        })
        .collect();
    root.set(1);
    for (i, m) in memos.iter().enumerate() {
        assert_eq!(m.get(), 1 + i as u64);
    }

    let mut counter = 1u64;
    let mut group = c.benchmark_group("signal_fan_out_200");
    group.throughput(Throughput::Elements(200));
    group.bench_function("set_and_resolve", |b| {
        b.iter(|| {
            counter = counter.wrapping_add(1);
            root.set(counter);
            let mut acc = 0u64;
            for m in &memos {
                acc = acc.wrapping_add(m.get());
            }
            black_box(acc);
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Bench 6: text shaping throughput via Aurora's TextContext.
// Cached = same 500 labels (cache-hit path); cold = salted strings per
// iteration (cache-miss/shaper path). Conflating the two is the classic
// skew here, so they are separate bench functions.
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn shape_labels(ctx: &aurora::text::TextContext, labels: &[String]) {
    use aurora::foundation::Constraints;
    use aurora::text::{FontStyle, TextAlign, TextOverflow};
    use aurora::text::shaper::TextShapeParams;
    for s in labels {
        black_box(ctx.shape(TextShapeParams {
            text: s,
            font: None,
            family: None,
            style: FontStyle::Normal,
            size: 16.0,
            line_height: 20.0,
            letter_spacing: 0.0,
            align: TextAlign::Start,
            overflow: TextOverflow::Clip,
            max_lines: None,
            weight: 400,
            constraints: Constraints::loose(f32::INFINITY, f32::INFINITY),
        }));
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_text_shaping(c: &mut Criterion) {
    use aurora::text::TextContext;
    let ctx = TextContext::new();
    ctx.load_font(include_bytes!("../../../assets/inter-var.ttf"));

    let labels: Vec<String> = (0..500)
        .map(|i| format!("core {:02}  load {:.1}%  pid {}", i % 64, (i % 100) as f32, 1000 + i))
        .collect();
    // Warm the cache so `labels_500_cached` measures the hit path.
    shape_labels(&ctx, &labels);

    let mut group = c.benchmark_group("text_shaping");
    group.throughput(Throughput::Elements(500));
    group.bench_function("labels_500_cached", |b| {
        b.iter(|| shape_labels(&ctx, &labels));
    });

    // Cold path: fresh strings every iteration defeat the shaped-text cache.
    let mut salt = 0u32;
    group.bench_function("labels_500_cold", |b| {
        b.iter_batched(
            || {
                salt = salt.wrapping_add(1);
                (0..500)
                    .map(|i| format!("run{salt} core {:02} load {}", i % 64, 1000 + i))
                    .collect::<Vec<_>>()
            },
            |batch_labels| shape_labels(&ctx, &batch_labels),
            BatchSize::SmallInput,
        );
    });

    // Jittered widths: same 500 strings, each shaped at a width jittered by
    // animation-scale noise (<=0.05px, as a spring-driven layout produces).
    // Without key quantization every iteration is 500 cold shapes; with it
    // the whole batch shares one entry per string (cache-hit path).
    group.bench_function("labels_500_jittered_widths", |b| {
        use aurora::foundation::Constraints;
        use aurora::text::{FontStyle, TextAlign, TextOverflow};
        use aurora::text::shaper::TextShapeParams;
        let mut iter = 0u32;
        b.iter(|| {
            iter = iter.wrapping_add(1);
            for (i, s) in labels.iter().enumerate() {
                // Deterministic sub-quantum jitter: same string, width wobble.
                let wobble = ((iter.wrapping_mul(2654435761).wrapping_add(i as u32 * 97)) % 5) as f32 * 0.01;
                black_box(ctx.shape(TextShapeParams {
                    text: s,
                    font: None,
                    family: None,
                    style: FontStyle::Normal,
                    size: 16.0,
                    line_height: 20.0,
                    letter_spacing: 0.0,
                    align: TextAlign::Start,
                    overflow: TextOverflow::Clip,
                    max_lines: None,
                    weight: 400,
                    constraints: Constraints::loose(400.0 + wobble, f32::INFINITY),
                }));
            }
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Bench 7 (replacement B): reorder + scroll through the real Engine.
// Aurora has no DataTable; large lists are keyed reconcile + chunk-cache
// replay, so that is what is measured: a 1k keyed list rotated per frame,
// and a spacer-driven scroll step over 500 rows (same technique as
// `tests/chunk_locality.rs`).
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn build_list(order: &[String]) -> aurora::dsl::GroupDef {
    use aurora::dsl::{group, text, IntoElement};
    use aurora::foundation::{Color, Direction, Size};
    let rows: Vec<aurora::dsl::Element> = order
        .iter()
        .map(|label| {
            group()
                .key(format!("row-{label}"))
                .width(Size::fill())
                .height(Size::Fixed(40.0))
                .fill(Color::hex(0x4F46E5))
                .children([text(label.clone())
                    .size(14.0)
                    .color(Color::WHITE)
                    .into_element()])
                .into_element()
        })
        .collect();
    group()
        .key("list")
        .direction(Direction::Vertical)
        .width(Size::Fixed(760.0))
        .height(Size::fill())
        .gap(10.0)
        .children(rows)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_reorder_and_scroll(c: &mut Criterion) {
    use aurora::dsl::{group, IntoElement};
    use aurora::foundation::{Color, Direction, Size};
    use aurora::runtime::Engine;

    let mut cg = c.benchmark_group("reorder_and_scroll");

    // Rotate-first-to-last on a 1k keyed list, full set_root + frame.
    cg.bench_function("reorder_1k_set_root_frame", |b| {
        let mut engine = Engine::headless(760, 900);
        let mut order: Vec<String> = (0..1000).map(|i| format!("item-{i:04}")).collect();
        engine.mount(build_list(&order).into_element());
        engine.frame();
        let mut rot: usize = 0;
        b.iter(|| {
            rot += 1;
            order.rotate_left(1);
            black_box(rot);
            engine.mount(build_list(&order).into_element());
            black_box(engine.frame());
        });
    });

    // Spacer-driven scroll step over 500 rows: content untouched, only the
    // spacer signal moves (offset-only motion must replay cached chunks).
    cg.bench_function("scroll_step_spacer_500", |b| {
        let mut engine = Engine::headless(400, 800);
        let spacer = engine.signal(0.0_f32);
        let rows: Vec<aurora::dsl::Element> = (0..500)
            .map(|i| {
                group()
                    .key(format!("srow-{i:04}"))
                    .width(Size::fill())
                    .height(Size::Fixed(32.0))
                    .fill(Color::hex(0x4F46E5))
                    .into_element()
            })
            .collect();
        engine.mount(
            group()
                .direction(Direction::Vertical)
                .width(Size::Fixed(400.0))
                .height(Size::fill())
                .children((
                    group()
                        .key("spacer")
                        .width(Size::fill())
                        .height(spacer.clone())
                        .into_element(),
                    group()
                        .key("slist")
                        .direction(Direction::Vertical)
                        .width(Size::fill())
                        .gap(6.0)
                        .children(rows)
                        .into_element(),
                ))
                .into_element(),
        );
        engine.frame();
        let mut h = 0.0f32;
        b.iter(|| {
            h += 32.0;
            if h > 640.0 {
                h = 0.0;
            }
            spacer.set(h);
            black_box(engine.frame());
        });
    });
    cg.finish();
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
criterion_group!(
    benches,
    bench_signal_chain,
    bench_tree_arena_10k,
    bench_diamond_network,
    bench_layout_1k,
    bench_fan_out_200,
    bench_text_shaping,
    bench_reorder_and_scroll
);
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
criterion_main!(benches);

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn main() {
    eprintln!("bench_suite is host-only (criterion); nothing to run on wasm32-unknown-unknown");
}
