// Single responsibility: Client-side responsive grid math over width signals.
//
// Deliberately *not* an engine feature: column count and cell width are pure
// arithmetic on an already-known available width (a window/container signal or
// a fixed box). The engine never parses `repeat(auto-fit, minmax(...))`; user
// code derives `(columns, child_width)` here and emits ordinary nested `Group`
// rows. See `examples/wrap_repro/view.rs` (`AutoGrid`) for the pattern.

/// Columns and per-cell width for `avail` px holding cells of at least
/// `min_child` px separated by `gap` px.
///
/// Mirrors `repeat(auto-fit, minmax(min_child, 1fr))`: fits as many
/// `min_child` tracks as possible (at least one), then distributes leftover
/// space evenly (the `1fr` part).
pub fn columns_for(avail: f32, min_child: f32, gap: f32) -> (u32, f32) {
    let avail = if avail.is_finite() { avail.max(0.0) } else { 0.0 };
    let min_child = if min_child.is_finite() {
        min_child.max(1.0)
    } else {
        1.0
    };
    let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
    let mut cols = ((avail + gap) / (min_child + gap)).floor() as u32;
    cols = cols.max(1);
    let child_w = if cols == 1 {
        avail
    } else {
        ((avail - gap * (cols - 1) as f32) / cols as f32).max(0.0)
    };
    (cols, child_w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_fit_math() {
        // 800px, min 250, gap 0 → 3 tracks of 266.67 (3×250=750 + 50 leftover).
        let (cols, w) = columns_for(800.0, 250.0, 0.0);
        assert_eq!(cols, 3);
        assert!((w - 800.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn shrinks_to_fewer_tracks() {
        let (cols, _) = columns_for(700.0, 250.0, 16.0);
        assert_eq!(cols, 2);
    }

    #[test]
    fn never_zero_columns() {
        let (cols, w) = columns_for(100.0, 250.0, 8.0);
        assert_eq!(cols, 1);
        assert_eq!(w, 100.0);
    }
}
