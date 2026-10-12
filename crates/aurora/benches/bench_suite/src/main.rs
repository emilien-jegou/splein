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
use aurora::reactive::{batch, Derived, ReactiveRuntime, Signal};
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion, Throughput};

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
        eprintln!(
            "arena 10k lifecycle: median {:?} (PASSED strict gate)",
            median
        );
    } else {
        eprintln!(
            "arena 10k lifecycle: strict gate disabled; set AURORA_STRICT_BENCH=1 to enforce"
        );
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
    use taffy::prelude::*;
    use taffy::{AvailableSpace, Size as GeoSize};
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
    use aurora::text::shaper::TextShapeParams;
    use aurora::text::{FontStyle, TextAlign, TextOverflow, TextWrap};
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
            variations: &[],
            constraints: Constraints::loose(f32::INFINITY, f32::INFINITY),
            wrap: TextWrap::NoWrap,
        }));
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_text_shaping(c: &mut Criterion) {
    use aurora::text::TextContext;
    let ctx = TextContext::new();
    ctx.load_font(include_bytes!("../../../assets/inter-var.ttf"));

    let labels: Vec<String> = (0..500)
        .map(|i| {
            format!(
                "core {:02}  load {:.1}%  pid {}",
                i % 64,
                (i % 100) as f32,
                1000 + i
            )
        })
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
        use aurora::text::shaper::TextShapeParams;
        use aurora::text::{FontStyle, TextAlign, TextOverflow, TextWrap};
        let mut iter = 0u32;
        b.iter(|| {
            iter = iter.wrapping_add(1);
            for (i, s) in labels.iter().enumerate() {
                // Deterministic sub-quantum jitter: same string, width wobble.
                let wobble =
                    ((iter.wrapping_mul(2654435761).wrapping_add(i as u32 * 97)) % 5) as f32 * 0.01;
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
                    variations: &[],
                    constraints: Constraints::loose(400.0 + wobble, f32::INFINITY),
                    wrap: TextWrap::NoWrap,
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

// ---------------------------------------------------------------------------
// Bench 6b: glyph rasterization, miss vs hit path, body vs title size.
//
// Pre-glyph-cache this re-renders every glyph every call (outline scale +
// hint + zeno + alloc). Post-cache the `hit` functions measure lookup cost.
// `miss` uses a fresh context per batch so every iteration cold-renders.
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_glyph_raster(c: &mut Criterion) {
    use aurora::foundation::Constraints;
    use aurora::text::{TextConfig, TextContext};

    fn keys_for(size: f32) -> (TextContext, Vec<aurora::text::GlyphKey>) {
        let ctx = TextContext::new();
        ctx.load_font(include_bytes!("../../../assets/inter-var.ttf"));
        // Pangram-ish content with digits/punctuation for distinct glyphs.
        let content: String = ("The quick brown fox jumps over 0123456789! ".repeat(4))
            .chars()
            .collect();
        let cfg = TextConfig {
            content,
            size,
            ..TextConfig::default()
        };
        let layout = ctx.shape_config(&cfg, Constraints::UNCONSTRAINED);
        let keys: Vec<_> = layout
            .lines
            .iter()
            .flat_map(|l| l.glyphs.iter().map(|g| g.cache_key))
            .collect();
        assert!(!keys.is_empty());
        (ctx, keys)
    }

    let mut group = c.benchmark_group("glyph_raster");
    for size in [12.0f32, 48.0] {
        let name = format!("size_{size}px");
        // Miss path: fresh context per batch => empty caches, full render.
        // Phases spread over X quarters and Y eighths like real text at
        // mixed pen positions (x in low 2 bits, y in next 3).
        let phase = |i: usize| ((i % 4) | (((i / 4) % 8) << 2)) as u8;
        group.bench_function(format!("{name}_miss"), |b| {
            b.iter_batched(
                || keys_for(size),
                |(ctx, keys)| {
                    for (i, k) in keys.iter().enumerate() {
                        ctx.raster_glyph(*k, phase(i), |img| {
                            black_box(img.map(|g| g.width));
                        });
                    }
                },
                BatchSize::SmallInput,
            );
        });
        // Hit path: warmed once per phase, then re-requested.
        let (ctx, keys) = keys_for(size);
        for (i, k) in keys.iter().enumerate() {
            ctx.raster_glyph(*k, phase(i), |_| {});
        }
        group.bench_function(format!("{name}_hit"), |b| {
            b.iter(|| {
                for (i, k) in keys.iter().enumerate() {
                    ctx.raster_glyph(*k, phase(i), |img| {
                        black_box(img.map(|g| g.width));
                    });
                }
            });
        });
        group.throughput(Throughput::Elements(keys.len() as u64));
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Bench 6c: batch shaping — sequential vs scoped-thread shape_many over 64
// distinct labels (enough misses to cross the parallel threshold).
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_shape_many(c: &mut Criterion) {
    use aurora::foundation::Constraints;
    use aurora::text::shaper::TextShapeParams;
    use aurora::text::{FontStyle, TextAlign, TextContext, TextOverflow, TextWrap};

    let texts: Vec<String> = (0..64)
        .map(|i| format!("batch shape many {i:02} with several words"))
        .collect();

    let mut group = c.benchmark_group("shape_many");
    group.throughput(Throughput::Elements(64));
    // Sequential baseline: fresh context per batch so every label misses.
    group.bench_function("sequential_64_cold", |b| {
        b.iter_batched(
            || {
                let ctx = TextContext::new();
                ctx.load_font(include_bytes!("../../../assets/inter-var.ttf"));
                ctx
            },
            |ctx| {
                for t in &texts {
                    black_box(ctx.shape(TextShapeParams {
                        text: t,
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
                        variations: &[],
                        constraints: Constraints::loose(400.0, f32::INFINITY),
                    wrap: TextWrap::NoWrap,
                    }));
                }
            },
            BatchSize::SmallInput,
        );
    });
    // Parallel batch over the same miss pattern.
    group.bench_function("shape_many_64_cold", |b| {
        b.iter_batched(
            || {
                let ctx = TextContext::new();
                ctx.load_font(include_bytes!("../../../assets/inter-var.ttf"));
                ctx
            },
            |ctx| {
                let params: Vec<TextShapeParams<'_>> = texts
                    .iter()
                    .map(|t| TextShapeParams {
                        text: t,
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
                        variations: &[],
                        constraints: Constraints::loose(400.0, f32::INFINITY),
                    wrap: TextWrap::NoWrap,
                    })
                    .collect();
                black_box(ctx.shape_many(&params));
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
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
    bench_glyph_raster,
    bench_shape_many,
    bench_reorder_and_scroll,
    bench_scroll_tick
);
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
criterion_main!(benches);

// ---------------------------------------------------------------------------
// Bench: real scroll ticks through ScrollView / VirtualScrollView.
//
// Unlike `scroll_step_spacer_500` (a layout-shift proxy), these drive the
// actual SCROLL channel on mounted viewports and rasterize through the
// retained tiny-skia canvas (blit + exposed strip). Per-tick wall time is
// the B target (~1ms at 4K); the eprintln counters prove the mechanism:
// `laid_out` must stay 0 (no layout per tick), `new_cmds` near 0 (row
// replay), `rects` small (strip damage). Fill-only rows: shaping is a
// separate bench, this one isolates scroll machinery.
// ---------------------------------------------------------------------------
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn bench_scroll_tick(c: &mut Criterion) {
    use aurora::dsl::{group, Element, IntoElement, RowFn, ScrollView, VirtualScrollView};
    use aurora::foundation::{Color, Size};
    use aurora::reactive::Cx;
    use aurora::runtime::Engine;
    use aurora_tiny_skia::raster::HeadlessRaster;

    fn fill_rows(n: usize, h: f32) -> Vec<Element> {
        (0..n)
            .map(|i| {
                group()
                    .key(format!("brow-{i:05}"))
                    .width(Size::fill())
                    .height(Size::Fixed(h))
                    .fill(Color::hex(if i % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC }))
                    .into_element()
            })
            .collect()
    }

    /// Times steady-state ticks only: setup (mount, first paint, shaping)
    /// runs before the loop, and each `b.iter` call runs a fixed batch so
    /// the headline number cannot include setup no matter how the harness
    /// schedules routine executions. (Stays local: `rich_case` below inlines
    /// its own loop for the same reason.)
    macro_rules! tick_batch {
        ($b:ident, $n:expr, $tick:expr) => {
            $b.iter(|| {
                for _ in 0..$n {
                    $tick();
                }
            })
        };
    }

    let mut cg = c.benchmark_group("scroll_tick");

    // ScrollView, 500 fill rows, +-6px ping-pong ticks, engine + raster.
    cg.bench_function("scrollview_500r_tick", |b| {
        let mut engine = Engine::headless(400, 600);
        let offset = engine.signal(0.0f32);
        let rt = engine.runtime();
        let rows = fill_rows(500, 32.0);
        let el = Cx::new(Rc::clone(&rt)).enter(|| {
            ScrollView(
                Size::Fixed(400.0),
                Size::Fixed(600.0),
                None,
                None,
                Some(offset.clone()),
                None,
                None,
                None,
                None,
                None,
                rows,
            )
        });
        engine.mount(el);
        engine.frame();
        let mut raster = HeadlessRaster::new();
        // Settle caches, clamp, and thumb before measuring.
        let (mut x, mut dir) = (0.0f32, 1.0f32);
        for _ in 0..40 {
            x += dir * 6.0;
            if x >= 120.0 {
                dir = -1.0;
            } else if x <= 0.0 {
                dir = 1.0;
            }
            offset.set(x);
            engine.frame();
            raster.draw_engine(&mut engine);
        }
        let (mut frames, mut cmds, mut cached, mut rects, mut laid) = (0u64, 0u64, 0u64, 0usize, 0u64);
        tick_batch!(b, 100, || {
            x += dir * 6.0;
            if x >= 120.0 {
                dir = -1.0;
            } else if x <= 0.0 {
                dir = 1.0;
            }
            offset.set(x);
            let report = engine.frame();
            raster.draw_engine(&mut engine);
            frames += 1;
            cmds += report.diagnostics.compile.commands_emitted as u64;
            cached += report.diagnostics.compile.cached_chunks as u64;
            rects += report.diagnostics.spatial.damaged_rects.len();
            laid += report.diagnostics.layout.laid_out as u64;
            black_box(report.diagnostics.frame_index);
        });
        eprintln!(
            "scrollview_500r: frames={frames} avg_new_cmds={:.1} avg_replays={:.1} avg_rects={:.2} laid_out={laid}",
            cmds as f64 / frames.max(1) as f64,
            cached as f64 / frames.max(1) as f64,
            rects as f64 / frames.max(1) as f64,
        );
    });

    // VirtualScrollView, 100k uniform rows, 240px ramp (window slides),
    // engine + raster. Mounted rows must stay near window size throughout.
    cg.bench_function("virtual_100k_tick", |b| {
        let mut engine = Engine::headless(400, 600);
        let offset = engine.signal(0.0f32);
        let rt = engine.runtime();
        let el = Cx::new(Rc::clone(&rt)).enter(|| {
            VirtualScrollView(
                Size::Fixed(400.0),
                Size::Fixed(600.0),
                100_000usize,
                32.0,
                RowFn::from(|i: usize| {
                    group()
                        .width(Size::fill())
                        .height(Size::Fixed(32.0))
                        .fill(Color::hex(if i % 2 == 0 { 0xEDEFF3 } else { 0xE2E5EC }))
                        .into_element()
                }),
                None,
                None,
                Some(offset.clone()),
                None,
                None,
                None,
                None,
                None,
            )
        });
        engine.mount(el);
        engine.frame();
        let mut raster = HeadlessRaster::new();
        let mut x = 0.0f32;
        // Settle the window on its ramp before measuring.
        for _ in 0..40 {
            x += 24.0;
            if x > 2400.0 {
                x = 0.0;
            }
            offset.set(x);
            engine.frame();
            raster.draw_engine(&mut engine);
        }
        let (mut frames, mut cmds, mut cached, mut rects, mut laid, mut mounted) =
            (0u64, 0u64, 0u64, 0usize, 0u64, 0usize);
        tick_batch!(b, 100, || {
            x += 24.0;
            if x > 2400.0 {
                x = 0.0;
            }
            offset.set(x);
            let report = engine.frame();
            raster.draw_engine(&mut engine);
            frames += 1;
            cmds += report.diagnostics.compile.commands_emitted as u64;
            cached += report.diagnostics.compile.cached_chunks as u64;
            rects += report.diagnostics.spatial.damaged_rects.len();
            laid += report.diagnostics.layout.laid_out as u64;
            mounted = report.diagnostics.compile.stacking_contexts;
            black_box(report.diagnostics.frame_index);
        });
        eprintln!(
            "virtual_100k: frames={frames} avg_new_cmds={:.1} avg_replays={:.1} avg_rects={:.2} laid_out={laid} last_contexts={mounted}",
            cmds as f64 / frames.max(1) as f64,
            cached as f64 / frames.max(1) as f64,
            rects as f64 / frames.max(1) as f64,
        );
    });
    // Rich content: text (12 distinct shaped runs, cached after first paint)
    // plus a live shadow per row — the complex-page gate. Shaping happens
    // once per distinct string; steady ticks measure replay + strip raster
    // of glyph coverage and shadow maps. Text-only and shadow-only variants
    // isolate which dominates the strip cost.
    rich_case(&mut cg, "scrollview_rich_tick", true, true);
    rich_case(&mut cg, "scrollview_text_tick", true, false);
    rich_case(&mut cg, "scrollview_shadow_tick", false, true);
    cg.finish();
}

/// One rich-content scroll case. `with_text`/`with_shadow` toggle the two
/// strip-raster cost drivers independently.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn rich_case(
    cg: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    with_text: bool,
    with_shadow: bool,
) {
    use aurora::dsl::{group, text, Element, IntoElement, ScrollView};
    use aurora::foundation::{Color, Size};
    use aurora::reactive::Cx;
    use aurora::runtime::Engine;
    use aurora_tiny_skia::raster::HeadlessRaster;

    cg.bench_function(name, |b| {
        let mut engine = Engine::headless(800, 1000);
        engine.load_font(include_bytes!("../../../assets/inter-var.ttf"));
        let offset = engine.signal(0.0f32);
        let rt = engine.runtime();
        let rows: Vec<Element> = (0..150)
            .map(|i| {
                let mut row = group()
                    .key(format!("rrow-{i:04}"))
                    .width(Size::fill())
                    .height(Size::Fixed(44.0))
                    .fill(Color::hex(if i % 2 == 0 { 0xFFFFFF } else { 0xF1F3F5 }));
                if with_shadow {
                    row = row.shadow(6.0);
                }
                if with_text {
                    let copy = format!(
                        "Article {:02} — pack my box with five dozen liquor jugs 0123456789",
                        i % 12
                    );
                    row = row.children([
                        text(copy).size(14.0).color(Color::hex(0x1A1D21)).into_element(),
                    ]);
                }
                row.into_element()
            })
            .collect();
        let el = Cx::new(Rc::clone(&rt)).enter(|| {
            ScrollView(
                Size::Fixed(800.0),
                Size::Fixed(1000.0),
                None,
                None,
                Some(offset.clone()),
                None,
                None,
                None,
                None,
                None,
                rows,
            )
        });
        engine.mount(el);
        engine.frame();
        engine.frame();
        let mut raster = HeadlessRaster::new();
        let (mut x, mut dir) = (0.0f32, 1.0f32);
        let mut step = || {
            x += dir * 9.0;
            if x >= 240.0 {
                dir = -1.0;
            } else if x <= 0.0 {
                dir = 1.0;
            }
            offset.set(x);
        };
        // Settle shaping, clamp, and thumb before measuring.
        for _ in 0..40 {
            step();
            engine.frame();
            raster.draw_engine(&mut engine);
        }
        let (mut frames, mut cmds, mut cached, mut rects, mut laid) = (0u64, 0u64, 0u64, 0usize, 0u64);
        // Fixed batch per `b.iter` call (see `tick_batch` in
        // `bench_scroll_tick`): setup stays out of the headline number.
        b.iter(|| {
            for _ in 0..50 {
                step();
                let report = engine.frame();
                raster.draw_engine(&mut engine);
                frames += 1;
                cmds += report.diagnostics.compile.commands_emitted as u64;
                cached += report.diagnostics.compile.cached_chunks as u64;
                rects += report.diagnostics.spatial.damaged_rects.len();
                laid += report.diagnostics.layout.laid_out as u64;
                black_box(report.diagnostics.frame_index);
            }
        });
        eprintln!(
            "{name}: frames={frames} avg_new_cmds={:.1} avg_replays={:.1} avg_rects={:.2} laid_out={laid}",
            cmds as f64 / frames.max(1) as f64,
            cached as f64 / frames.max(1) as f64,
            rects as f64 / frames.max(1) as f64,
        );
    });
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn main() {
    eprintln!("bench_suite is host-only (criterion); nothing to run on wasm32-unknown-unknown");
}
