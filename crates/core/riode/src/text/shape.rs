// Single responsibility: Parley ranged-builder shaping into Aurora text layouts.

#![cfg_attr(not(feature = "text-runtime"), allow(dead_code))]
use std::cell::RefCell;

use crate::foundation::{Constraints, IntrinsicSize};
use crate::text::align::apply_alignment;
use crate::text::engine::ParleyEngine;
use crate::text::fonts::FontStyle;
use crate::text::layout::TextLayout;
use crate::text::wrap::TextWrap;
use crate::text::shaper::TextShapeParams;
#[cfg(feature = "text-runtime")]
use crate::text::runs::extract_lines;
#[cfg(feature = "text-runtime")]
use parley::{
    FontFamily, FontStyle as ParleyStyle, FontWeight, Layout, LayoutContext, LineHeight,
    OverflowWrap, StyleProperty, TextWrapMode,
};

// Scratch layout space, one per thread: Parley's documented sharing
// granularity. A single global context behind a mutex would serialize every
// shaping thread; thread-local contexts remove that serial section while the
// shared font collection stays behind its (shorter-lived) lock.
#[cfg(feature = "text-runtime")]
thread_local! {
    static LAYOUT_CTX: RefCell<LayoutContext<()>> = RefCell::new(LayoutContext::new());
}

/// Shapes a single string into a multi-line layout against the engine font database.
///
/// Parley ingests letter spacing natively, so one shaping pass suffices: the
/// old iterative width-compensation loop for cosmic-text is gone.
///
/// Line endings are normalized before shaping: Parley breaks `\r` and `\n`
/// independently, so Windows `\r\n` input would render a phantom blank line
/// per break. Cluster offsets are remapped back to the original bytes, since
/// selection and ellipsis slicing index the caller's text, not the copy.
#[cfg(feature = "text-runtime")]
pub(crate) fn shape_text(
    engine: &ParleyEngine,
    fcx: &mut parley::FontContext,
    text: &str,
    params: &TextShapeParams,
) -> TextLayout {
    let mut target = wrap_width(params.constraints);
    // Balanced breaking is a width search, not a Parley mode (0.11 has none):
    // pick the wrap width with the most even lines, then shape once there.
    // The offered width stays the cache key (see `TextContext::shape`), so the
    // candidates below never pollute or thrash the shared cache — they shape
    // directly and only on a cache miss of the offered width.
    if params.wrap.is_balance() {
        if let Some(avail) = target {
            target = Some(balance_best_width(engine, fcx, text, params, avail));
        }
    }
    if text.bytes().any(|b| b == b'\r') {
        let (normalized, new_to_old) = normalize_line_endings(text);
        let mut layout = shape_at_width(engine, fcx, &normalized, params, target);
        let sentinel = text.len();
        for line in &mut layout.lines {
            for g in &mut line.glyphs {
                g.cluster = new_to_old
                    .get(g.cluster)
                    .copied()
                    .unwrap_or(sentinel)
                    .min(sentinel);
            }
        }
        return layout;
    }
    shape_at_width(engine, fcx, text, params, target)
}

/// Normalizes line endings (`\r\n` and lone `\r` become `\n`), returning the
/// normalized text plus a map from each normalized byte offset (and one past
/// the end) to the corresponding original byte offset.
///
/// ASCII-only transformation: `\r` and `\n` are single bytes that never appear
/// inside multibyte sequences, so offsets stay on char boundaries.
#[cfg(feature = "text-runtime")]
fn normalize_line_endings(text: &str) -> (String, Vec<usize>) {
    let bytes = text.as_bytes();
    let mut normalized = String::with_capacity(text.len());
    let mut new_to_old = Vec::with_capacity(text.len() + 1);
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            // `\r\n` collapses to one `\n` attributed to the `\r` position;
            // either original byte is safe for cluster slicing (both ASCII).
            new_to_old.push(i);
            normalized.push('\n');
            i += 1;
            if bytes.get(i) == Some(&b'\n') {
                i += 1;
            }
        } else {
            // Copy one full char: `\r`/`\n` are ASCII, so any other byte
            // starts a char boundary in valid UTF-8 input.
            let ch_len = text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            for k in 0..ch_len {
                new_to_old.push(i + k);
            }
            normalized.push_str(&text[i..i + ch_len]);
            i += ch_len;
        }
    }
    new_to_old.push(text.len());
    debug_assert_eq!(new_to_old.len(), normalized.len() + 1);
    (normalized, new_to_old)
}

/// Finite positive wrap width from constraints, or None when unconstrained.
#[cfg(feature = "text-runtime")]
fn wrap_width(constraints: Constraints) -> Option<f32> {
    if constraints.max_width.is_finite() && constraints.max_width > 0.0 {
        Some(constraints.max_width)
    } else {
        None
    }
}

/// Sanitizes tracking into a finite, non-negative interval.
#[cfg(feature = "text-runtime")]
fn safe_spacing(spacing: f32) -> f32 {
    if spacing.is_finite() && spacing > 0.0 {
        spacing
    } else {
        0.0
    }
}

/// Shapes the string at one wrap width, returning tracked lines and total size.
///
/// Takes the font context by mutable reference: single-shape callers pass the
/// shared collection under its lock; batch workers pass a thread-local clone.
/// Either way the layout scratch comes from the calling thread's `LAYOUT_CTX`.
#[cfg(feature = "text-runtime")]
fn shape_at_width(
    engine: &ParleyEngine,
    fcx: &mut parley::FontContext,
    text: &str,
    params: &TextShapeParams,
    wrap: Option<f32>,
) -> TextLayout {
    let safe_size = if params.size.is_finite() && params.size > 0.0 {
        params.size
    } else {
        14.0
    };
    let safe_lh = if params.line_height.is_finite() && params.line_height > 0.0 {
        params.line_height
    } else {
        safe_size * 1.2
    };
    let spacing = safe_spacing(params.letter_spacing);

    let style = match params.style {
        FontStyle::Normal => ParleyStyle::Normal,
        FontStyle::Italic => ParleyStyle::Italic,
        FontStyle::Oblique => ParleyStyle::Oblique(None),
    };
    let family_name = engine.resolve_family(fcx, params.font, params.family);
    // Owned variation settings: the builder only borrows them during `build`.
    let parley_variations: Vec<parley::FontVariation> =
        params.variations.iter().map(|v| v.to_parley()).collect();

    // The built layout owns its data: the font context is released before
    // line extraction so remaining locks are never stacked inside it. The
    // `&mut` reborrows below keep the caller's context usable afterwards.
    let layout: Layout<()> = LAYOUT_CTX.with(|lcx| {
        let mut lcx = lcx.borrow_mut();
        let mut builder = lcx.ranged_builder(fcx, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(safe_size));
        builder.push_default(StyleProperty::LineHeight(LineHeight::Absolute(safe_lh)));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            params.weight as f32,
        )));
        builder.push_default(StyleProperty::FontStyle(style));
        // Emergency mid-word breaks when a word alone exceeds the wrap width,
        // matching the old cosmic-text wrapping contract (only used if needed).
        builder.push_default(StyleProperty::OverflowWrap(OverflowWrap::Anywhere));
        // Refusing to wrap is a real Parley mode; balance is resolved by the
        // caller into a concrete width before reaching this builder.
        if params.wrap.is_nowrap() {
            builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
        }
        if spacing > 0.0 {
            builder.push_default(StyleProperty::LetterSpacing(spacing));
        }
        if let Some(name) = family_name.as_deref() {
            builder.push_default(StyleProperty::FontFamily(FontFamily::named(name)));
        }
        if !parley_variations.is_empty() {
            builder.push_default(StyleProperty::FontVariations(
                parley_variations.as_slice().into(),
            ));
        }
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(wrap);
        layout
    });

    let (mut lines, max_line_w) = extract_lines(engine, &layout, text, safe_lh, safe_size);
    // Alignment is baked in against the final wrap width, so the cached layout stays
    // immutable and can be shared as an `Arc` without per-frame glyph mutation.
    if let Some(box_width) = wrap {
        apply_alignment(&mut lines, box_width, params.align, text);
    }
    let total_h = (lines.len() as f32 * safe_lh).max(safe_lh);
    TextLayout::new(
        lines,
        IntrinsicSize {
            width: max_line_w,
            height: total_h,
        },
    )
}

/// Balanced wrap width for headings: the candidate width in
/// `[min_content, avail]` whose greedy break has the most even line lengths.
///
/// Only runs when the greedy break at `avail` yields 2–6 lines (heading
/// scope); otherwise returns `avail` untouched. Candidates shape directly
/// (bypassing the shared cache) with `Wrap` semantics to avoid recursion;
/// the caller caches the final balanced layout under the offered width, so
/// this search runs only on a cache miss. Score is line-length raggedness
/// (`max − min`); ties prefer the wider width for reflow stability.
#[cfg(feature = "text-runtime")]
fn balance_best_width(
    engine: &ParleyEngine,
    fcx: &mut parley::FontContext,
    text: &str,
    params: &TextShapeParams,
    avail: f32,
) -> f32 {
    const MAX_BALANCED_LINES: usize = 6;
    const CANDIDATES: usize = 10;

    let mut probe = *params;
    probe.wrap = TextWrap::Wrap;
    let greedy = shape_at_width(
        engine,
        fcx,
        text,
        &probe,
        Some(avail),
    );
    let target_count = greedy.lines.len();
    if target_count < 2 || target_count > MAX_BALANCED_LINES {
        return avail;
    }
    let unbounded = shape_at_width(engine, fcx, text, &probe, None);
    let min_w = crate::text::min_content::min_inline_width(text, &unbounded.lines)
        .min(avail)
        .max(1.0);
    // Degenerate (empty or single unbreakable run): nothing to even out.
    if min_w >= avail - 0.5 {
        return avail;
    }

    let mut best_w = avail;
    let mut best_ragged = line_raggedness(&greedy);
    for i in 0..CANDIDATES {
        let w = min_w + (avail - min_w) * (i as f32) / (CANDIDATES as f32);
        let layout = shape_at_width(engine, fcx, text, &probe, Some(w));
        if layout.lines.len() != target_count || layout.lines.len() < 2 {
            continue;
        }
        let ragged = line_raggedness(&layout);
        // Strictly-better only: ties keep the wider (earlier-found, since
        // candidates ascend) width.
        if ragged < best_ragged - 0.5 {
            best_ragged = ragged;
            best_w = w;
        }
    }
    best_w
}

/// Spread between the longest and shortest line of a layout.
#[cfg(feature = "text-runtime")]
fn line_raggedness(layout: &TextLayout) -> f32 {
    let mut min = f32::INFINITY;
    let mut max = 0.0_f32;
    for line in &layout.lines {
        let w: f32 = line.glyphs.iter().map(|g| g.advance).sum();
        min = min.min(w);
        max = max.max(w);
    }
    if min.is_finite() {
        (max - min).max(0.0)
    } else {
        0.0
    }
}

// ── Stub shaping when `text-runtime` is off: empty layout, no fonts. ──
#[cfg(not(feature = "text-runtime"))]
pub(crate) fn shape_text(
    _engine: &crate::text::engine::ParleyEngine,
    _fcx: &mut (),
    _text: &str,
    _params: &crate::text::shaper::TextShapeParams,
) -> crate::text::layout::TextLayout {
    crate::text::layout::TextLayout::new(
        Vec::new(),
        crate::foundation::IntrinsicSize::default(),
    )
}
