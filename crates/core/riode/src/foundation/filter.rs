// Single responsibility: CSS-style element filter specification with color-matrix helpers.

/// One filter function, CSS `filter` shorthand parity.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum FilterOp {
    /// Gaussian blur; the value is the standard deviation in px
    /// (CSS `blur(<length>)` semantics — the parameter IS sigma).
    Blur(f32),
    /// Multiplies RGB by the amount; 1.0 is identity (CSS `brightness()`).
    Brightness(f32),
    /// Adjusts saturation via the CSS luminance matrix; 1.0 is identity,
    /// 0.0 is grayscale (CSS `saturate()`).
    Saturate(f32),
    /// Scales RGB around 0.5; 1.0 is identity (CSS `contrast()`).
    Contrast(f32),
    /// Rotates hue in degrees (CSS `hue-rotate()`).
    HueRotate(f32),
    /// Raw row-major 4x5 matrix (effect-stack lowering): carries a
    /// pre-combined matrix that no single CSS op expresses.
    Matrix([f32; 20]),
}

impl FilterOp {
    /// CSS filter-effects color matrix (4x5, row-major: R, G, B, A rows
    /// each `[m0 m1 m2 m3 m4]` with m4 the constant offset). Applied to
    /// straight (non-premultiplied) RGBA in 0..1.
    pub fn color_matrix(self) -> Option<[f32; 20]> {
        let m = match self {
            // Blur has no color matrix (handled as a separate spatial pass).
            Self::Blur(_) => return None,
            Self::Matrix(m) => m,
            Self::Brightness(b) => [
                b, 0.0, 0.0, 0.0, 0.0, //
                0.0, b, 0.0, 0.0, 0.0, //
                0.0, 0.0, b, 0.0, 0.0, //
                0.0, 0.0, 0.0, 1.0, 0.0,
            ],
            Self::Saturate(s) => {
                // CSS/SVG saturate matrix (sRGB luminance coefficients).
                let (r, g, b) = (0.213f32, 0.715f32, 0.072f32);
                [
                    r + (1.0 - r) * s,
                    g - g * s,
                    b - b * s,
                    0.0,
                    0.0,
                    r - r * s,
                    g + (1.0 - g) * s,
                    b - b * s,
                    0.0,
                    0.0,
                    r - r * s,
                    g - g * s,
                    b + (1.0 - b) * s,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                ]
            }
            Self::Contrast(c) => {
                let o = 0.5 * (1.0 - c);
                [
                    c, 0.0, 0.0, 0.0, o, //
                    0.0, c, 0.0, 0.0, o, //
                    0.0, 0.0, c, 0.0, o, //
                    0.0, 0.0, 0.0, 1.0, 0.0,
                ]
            }
            Self::HueRotate(deg) => {
                // CSS hue-rotate: rotation matrix in YIQ space.
                let a = deg.to_radians();
                let (s, c) = (a.sin(), a.cos());
                [
                    0.213 + c * 0.787 - s * 0.213,
                    0.715 - c * 0.715 - s * 0.715,
                    0.072 - c * 0.072 + s * 0.928,
                    0.0,
                    0.0,
                    0.213 - c * 0.213 + s * 0.143,
                    0.715 + c * 0.285 + s * 0.140,
                    0.072 - c * 0.072 - s * 0.283,
                    0.0,
                    0.0,
                    0.213 - c * 0.213 - s * 0.787,
                    0.715 - c * 0.715 + s * 0.715,
                    0.072 + c * 0.928 + s * 0.072,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                ]
            }
        };
        Some(m)
    }
}

/// An ordered chain of filter functions (CSS `filter` list).
///
/// The engines resolve the chain into at most two passes: one spatial
/// blur at the largest sigma, then one combined color matrix. That folds
/// `brightness(2) saturate(0.5)` into a single matrix multiply but treats
/// blur as pre-multiplied first — fine for the common single-filter and
/// color-only cases; interleaved blur/color order is a documented v1
/// simplification.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Filter {
    ops: Vec<FilterOp>,
}

impl Filter {
    pub const EMPTY: Self = Self { ops: Vec::new() };

    /// Raw op chain (effect-stack lowering): blur sigmas as `Blur` ops,
    /// pre-combined color stages as `Matrix` ops, in execution order.
    pub fn from_ops(ops: Vec<FilterOp>) -> Self {
        Self { ops }
    }

    /// This chain as an effect stack: one `Blur` stage per blur op, then
    /// a single `ColorMatrix` stage for the combined color matrix (the
    /// engine fold order). Migration helper from `filter={f}` to
    /// `effects={f.into_effects()}`.
    pub fn into_effects(self) -> crate::foundation::EffectStack {
        use crate::foundation::{Blur, ColorMatrix, Effect};
        let mut items: Vec<Box<dyn Effect>> = Vec::new();
        for op in &self.ops {
            if let FilterOp::Blur(s) = op {
                items.push(Blur::new(*s).boxed());
            }
        }
        if let Some(m) = self.color_matrix() {
            items.push(ColorMatrix::new(m).boxed());
        }
        crate::foundation::EffectStack::from(items)
    }

    /// CSS `blur(<length>)`; `sigma` is the Gaussian standard deviation
    /// in px (the CSS parameter, not a box-shadow-style diameter).
    pub fn blur(sigma: f32) -> Self {
        Self {
            ops: vec![FilterOp::Blur(sigma.max(0.0))],
        }
    }

    pub fn brightness(amount: f32) -> Self {
        Self {
            ops: vec![FilterOp::Brightness(amount)],
        }
    }

    pub fn saturate(amount: f32) -> Self {
        Self {
            ops: vec![FilterOp::Saturate(amount)],
        }
    }

    pub fn contrast(amount: f32) -> Self {
        Self {
            ops: vec![FilterOp::Contrast(amount)],
        }
    }

    pub fn hue_rotate(degrees: f32) -> Self {
        Self {
            ops: vec![FilterOp::HueRotate(degrees)],
        }
    }

    /// Appends one more filter function (CSS list order).
    pub fn with(mut self, op: FilterOp) -> Self {
        self.ops.push(op);
        self
    }

    /// Appends a blur to an existing chain: `Filter::saturate(0.5).blur(4.0)`.
    pub fn blur_after(mut self, sigma: f32) -> Self {
        self.ops.push(FilterOp::Blur(sigma.max(0.0)));
        self
    }

    pub fn ops(&self) -> &[FilterOp] {
        &self.ops
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Largest blur sigma in the chain, for damage/visual bounds padding.
    /// The engines pad by `3 * sigma` (Gaussian support).
    pub fn max_blur_sigma(&self) -> f32 {
        self.ops
            .iter()
            .filter_map(|op| match op {
                FilterOp::Blur(s) => Some(*s),
                _ => None,
            })
            .fold(0.0, f32::max)
    }

    pub fn has_blur(&self) -> bool {
        self.max_blur_sigma() > 0.0
    }

    /// Combines every color op into one row-major 4x5 matrix, or `None`
    /// when the chain is empty or contains only blurs.
    pub fn color_matrix(&self) -> Option<[f32; 20]> {
        let mut acc: Option<[f32; 20]> = None;
        for op in &self.ops {
            let Some(m) = op.color_matrix() else {
                continue;
            };
            acc = Some(match acc {
                None => m,
                Some(prev) => mat4x5_mul(&m, &prev),
            });
        }
        acc
    }
}

/// Row-major 4x5 affine color-matrix composition: `a` applied after `b`
/// (i.e. the caller builds `acc = next * acc` so the first op in the
/// chain ends up applied first).
fn mat4x5_mul(a: &[f32; 20], b: &[f32; 20]) -> [f32; 20] {
    let mut out = [0.0f32; 20];
    for row in 0..4 {
        for col in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[row * 5 + k] * b[k * 5 + col];
            }
            out[row * 5 + col] = sum;
        }
        // Constant term: A's offset plus A's linear part times B's offset.
        let mut c = a[row * 5 + 4];
        for k in 0..4 {
            c += a[row * 5 + k] * b[k * 5 + 4];
        }
        out[row * 5 + 4] = c;
    }
    out
}

/// Parses one `name(value)` token: `blur(4px)`, `brightness(1.2)`,
/// `saturate(.5)`, `contrast(110%)`, `hue-rotate(90deg)`.
fn parse_filter_op(token: &str) -> Option<FilterOp> {
    let token = token.trim();
    let (name, rest) = token.split_once('(')?;
    let value = rest.trim_end_matches(')').trim();
    let name = name.trim().to_ascii_lowercase();
    let amount = parse_filter_amount(value)?;
    match name.as_str() {
        "blur" => Some(FilterOp::Blur(amount.max(0.0))),
        "brightness" => Some(FilterOp::Brightness(amount)),
        "saturate" => Some(FilterOp::Saturate(amount.max(0.0))),
        "contrast" => Some(FilterOp::Contrast(amount.max(0.0))),
        "hue-rotate" | "hue_rotate" => Some(FilterOp::HueRotate(amount)),
        _ => None,
    }
}

/// CSS filter amount: a plain number, `12px`, or `110%`.
fn parse_filter_amount(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(px) = s.strip_suffix("px") {
        return px.trim().parse::<f32>().ok();
    }
    if let Some(pct) = s.strip_suffix('%') {
        return pct.trim().parse::<f32>().ok().map(|v| v / 100.0);
    }
    if let Some(deg) = s.strip_suffix("deg") {
        return deg.trim().parse::<f32>().ok();
    }
    if let Some(rad) = s.strip_suffix("rad") {
        return rad.trim().parse::<f32>().ok().map(|v| v.to_degrees());
    }
    s.parse::<f32>().ok()
}

/// Parses a CSS `filter` shorthand: `blur(4px)`, `brightness(1.2)
/// saturate(0.5)`, or the empty string (no filter). Whitespace-separated
/// functions, applied left to right.
pub fn parse_filter(s: &str) -> Option<Filter> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") {
        return Some(Filter::EMPTY);
    }
    // Split on whitespace that sits between `)` and the next name —
    // values never contain spaces in our grammar.
    let mut ops = Vec::new();
    for token in s.split_whitespace() {
        ops.push(parse_filter_op(token)?);
    }
    if ops.is_empty() {
        return None;
    }
    Some(Filter { ops })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-4, "{a} !≈ {b}");
    }

    #[test]
    fn parse_single_and_chain() {
        let f = parse_filter("blur(4px)").unwrap();
        assert_eq!(f.ops(), &[FilterOp::Blur(4.0)]);
        let f = parse_filter("brightness(1.2) saturate(0.5)").unwrap();
        assert_eq!(f.ops(), &[FilterOp::Brightness(1.2), FilterOp::Saturate(0.5)]);
        let f = parse_filter("hue-rotate(90deg)").unwrap();
        assert_eq!(f.ops(), &[FilterOp::HueRotate(90.0)]);
        let f = parse_filter("contrast(110%)").unwrap();
        assert_eq!(f.ops(), &[FilterOp::Contrast(1.1)]);
        assert!(parse_filter("").unwrap().is_empty());
        assert!(parse_filter("none").unwrap().is_empty());
        assert!(parse_filter("bogus(1)").is_none());
    }

    #[test]
    fn builders_and_bounds() {
        let f = Filter::blur(4.0).with(FilterOp::Saturate(0.5));
        assert!(f.has_blur());
        approx(f.max_blur_sigma(), 4.0);
        assert!(!Filter::saturate(2.0).has_blur());
        approx(Filter::saturate(2.0).max_blur_sigma(), 0.0);
    }

    #[test]
    fn color_matrix_identity_and_combine() {
        // Brightness(1) is identity.
        let m = FilterOp::Brightness(1.0).color_matrix().unwrap();
        approx(m[0], 1.0);
        approx(m[6], 1.0);
        approx(m[12], 1.0);
        // brightness(2) then saturate(0): CSS applies left-to-right, so
        // grayscale sees the already-doubled color — each RGB row sums to
        // 2 * (luma weights) = 2.0, i.e. white in -> 2.0 gray out.
        let f = Filter::brightness(2.0).with(FilterOp::Saturate(0.0));
        let m = f.color_matrix().unwrap();
        let row_r: f32 = m[0] + m[1] + m[2];
        approx(row_r, 2.0);
        // All three rows identical: grayscale.
        approx(m[0], m[5]);
        approx(m[5], m[10]);
        // Hue-rotate(360) ~ identity.
        let m = FilterOp::HueRotate(360.0).color_matrix().unwrap();
        approx(m[0], 1.0);
        approx(m[6], 1.0);
        approx(m[12], 1.0);
    }

    #[test]
    fn blur_only_matrix_is_none() {
        assert!(Filter::blur(4.0).color_matrix().is_none());
        assert!(Filter::blur(4.0).with(FilterOp::Brightness(2.0)).color_matrix().is_some());
    }
}
