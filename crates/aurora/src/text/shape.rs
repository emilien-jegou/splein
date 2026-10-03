// Single responsibility: cosmic-text buffer shaping into Aurora text layouts.

use crate::foundation::{Constraints, IntrinsicSize};
use crate::text::align::apply_alignment;
use crate::text::buffer_pool::BufferKey;
use crate::text::cosmic::CosmicTextEngine;
use crate::text::fonts::{FontId, FontStyle};
use crate::text::layout::TextLayout;
use crate::text::runs::extract_lines;
use crate::text::shaper::TextShapeParams;
use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping, Stretch, Style, Weight};

/// Shapes a single string into a multi-line layout against the engine font database.
pub(crate) fn shape_text(
    engine: &CosmicTextEngine,
    text: &str,
    params: &TextShapeParams,
) -> TextLayout {
    let spacing = safe_spacing(params.letter_spacing);
    let Some(target) = wrap_width(params.constraints) else {
        return shape_at_width(engine, text, params, None);
    };
    if spacing <= 0.0 {
        return shape_at_width(engine, text, params, Some(target));
    }

    // cosmic-text 0.12 cannot ingest letter spacing, so a line's tracked width can exceed
    // the wrap width. Compensate by shaping at a reduced width until no line overflows.
    let mut width = target;
    let mut layout = shape_at_width(engine, text, params, Some(width));
    for _ in 0..3 {
        let overflow = layout
            .lines
            .iter()
            .map(|line| line.width - target)
            .fold(0.0_f32, f32::max);
        if overflow <= 0.01 {
            break;
        }
        width = (width - overflow).max(1.0);
        layout = shape_at_width(engine, text, params, Some(width));
        if width <= 1.0 {
            break;
        }
    }
    layout
}

/// Finite positive wrap width from constraints, or None when unconstrained.
fn wrap_width(constraints: Constraints) -> Option<f32> {
    if constraints.max_width.is_finite() && constraints.max_width > 0.0 {
        Some(constraints.max_width)
    } else {
        None
    }
}

/// Sanitizes tracking into a finite, non-negative interval.
fn safe_spacing(spacing: f32) -> f32 {
    if spacing.is_finite() && spacing > 0.0 {
        spacing
    } else {
        0.0
    }
}

/// Shapes the string at one wrap width, returning tracked lines and total size.
fn shape_at_width(
    engine: &CosmicTextEngine,
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
    let safe_spacing = safe_spacing(params.letter_spacing);

    let mut fs = engine.font_system.lock().unwrap();

    let style = match params.style {
        FontStyle::Normal => Style::Normal,
        FontStyle::Italic => Style::Italic,
        FontStyle::Oblique => Style::Oblique,
    };
    let style_code = match params.style {
        FontStyle::Normal => 0u8,
        FontStyle::Italic => 1,
        FontStyle::Oblique => 2,
    };
    let weight = Weight(params.weight);

    // Prefer the requested family, but always resolve to a family that exists so
    // shaping never panics with cosmic-text's "no default font found".
    let requested = params.family.map(str::to_string).or_else(|| {
        if let Some(FontId(id)) = params.font {
            fs.db()
                .faces()
                .nth(id as usize)
                .and_then(|f| f.families.first().map(|(name, _)| name.clone()))
        } else {
            None
        }
    });
    let family_name = match requested {
        Some(name)
            if fs
                .db()
                .faces()
                .any(|f| f.families.iter().any(|(n, _)| n == &name)) =>
        {
            Some(name)
        }
        _ => fs
            .db()
            .faces()
            .next()
            .and_then(|f| f.families.first().map(|(n, _)| n.clone())),
    };
    let family = family_name
        .as_deref()
        .map(Family::Name)
        .unwrap_or(Family::SansSerif);

    // cosmic-text only accepts an exact weight face for its default family and has no
    // variable-axis instancing, so relax to the family default when none matches.
    let has_exact_face = fs.db().faces().any(|f| {
        let family_ok = family_name
            .as_deref()
            .map_or(true, |name| f.families.iter().any(|(n, _)| n == name));
        family_ok && f.style == style && f.stretch == Stretch::Normal && f.weight.0 == params.weight
    });
    let attrs = if has_exact_face {
        Attrs::new().family(family).weight(weight).style(style)
    } else {
        Attrs::new().family(family)
    };

    // Reuse a retained buffer when only the wrap width changed; otherwise parse afresh.
    let key = BufferKey {
        content: text.into(),
        size_bits: safe_size.to_bits(),
        line_height_bits: safe_lh.to_bits(),
        weight: params.weight,
        style: style_code,
        family: family_name.clone().unwrap_or_default().into(),
    };
    let retained = {
        let mut pool = engine.buffers.lock().unwrap();
        pool.take(&key)
    };
    let mut buffer = match retained {
        Some(mut buffer) => {
            buffer.set_size(&mut fs, wrap, None);
            buffer
        }
        None => {
            let mut buffer = Buffer::new(&mut fs, Metrics::new(safe_size, safe_lh));
            buffer.set_size(&mut fs, wrap, None);
            buffer.set_text(&mut fs, text, attrs, Shaping::Advanced);
            buffer
        }
    };
    buffer.shape_until_scroll(&mut fs, false);

    let (mut lines, max_line_w) = extract_lines(engine, &buffer, safe_spacing, safe_lh, safe_size);
    engine.buffers.lock().unwrap().put(key, buffer);
    drop(fs);
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
