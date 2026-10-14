// Single responsibility: String/number literal inference for paint and layout values.
//
// Lets call sites write what they mean: `fill="#121212"`, `fill="rgb(18,18,18)"`,
// `fill={0x121212}`, `width="fill"`, `radius="max"`, `margin="8 16"`,
// `direction="vertical"`, `shadow="0 4 16 #0006"`.
//
// # The no-crash contract
//
// A literal means the same thing everywhere — on a primitive or inside a user
// component — and is validated nowhere at compile time (a `view!` that checked
// only the types it knows would be magic: same syntax, different rules). Every
// parser returns `Option`, so `FromStr` stays fallible; the `From<&str>` impls
// degrade to a sentinel and log loudly instead of panicking:
//
// | Bad literal for | Renders | Logs |
// |---|---|---|
// | color / fill | opaque magenta | value + grammar |
// | size | `Fit` | value + grammar |
// | radius | `ZERO` | value + grammar |
// | gap | `Fixed(0)` | value + grammar |
// | margin | `ZERO` | value + grammar |
// | direction / alignment / distribution | type default | value + grammar |
// | anchor | `Center` | value + grammar |
// | stroke | 1px magenta inside | value + grammar |
// | shadow | elevation 0 | value + grammar |
// | transition / ease / spring | snappy spring / `OutCubic` / snappy | value + grammar |
//
// The unwind boundaries (`component_scope`, `Engine::set_root`,
// `Engine::frame`) are the backstop for everything the sentinels miss: a bad
// frame is dropped and reported via `Engine::last_frame_error`, never propagated.

use std::str::FromStr;

use crate::foundation::alignment::{Alignment, Direction, Distribution, FlowWrap};
use crate::foundation::color::Color;
use crate::foundation::fill::Fill;
use crate::foundation::positioning::Anchor;
use crate::foundation::radius::Radius;
use crate::foundation::shadow::Shadow;
use crate::foundation::sizing::{AspectRatio, Size};
use crate::foundation::spacing::{Gap, Margin, Padding};
use crate::foundation::stroke::{Stroke, StrokeAlign};
use crate::text::TextWrap;

/// Accepted color grammar, for panic messages.
pub const COLOR_GRAMMAR: &str =
    "`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `121212` (bare hex), `rgb(r,g,b)` / `rgba(r,g,b,a)` (0-255 ints), or a name (transparent, black, white, red, green, blue, gray/grey, ink, paper, accent)";

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_hex_digits(s: &str) -> Option<Color> {
    let d: Vec<u8> = s.bytes().map(hex_val).collect::<Option<_>>()?;
    let dup = |n: u8| n * 16 + n;
    match d.as_slice() {
        [r, g, b] => Some(Color::rgba8(dup(*r), dup(*g), dup(*b), 255)),
        [r, g, b, a] => Some(Color::rgba8(dup(*r), dup(*g), dup(*b), dup(*a))),
        [r1, r2, g1, g2, b1, b2] => {
            Some(Color::rgba8(r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, 255))
        }
        [r1, r2, g1, g2, b1, b2, a1, a2] => Some(Color::rgba8(
            r1 * 16 + r2,
            g1 * 16 + g2,
            b1 * 16 + b2,
            a1 * 16 + a2,
        )),
        _ => None,
    }
}

fn parse_channel(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Ok(v @ 0..=255) = s.parse::<i32>() {
        return Some(v as f32 / 255.0);
    }
    s.parse::<f32>().ok().filter(|v| (0.0..=1.0).contains(v))
}

fn parse_rgb_fn(s: &str) -> Option<Color> {
    let s = s.trim();
    let (head, tail) = if let Some(t) = s.strip_prefix("rgba(") {
        ("rgba", t)
    } else if let Some(t) = s.strip_prefix("rgb(") {
        ("rgb", t)
    } else {
        return None;
    };
    let inner = tail.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').collect();
    match (head, parts.as_slice()) {
        ("rgb", [r, g, b]) => Some(Color::rgba(
            parse_channel(r)?,
            parse_channel(g)?,
            parse_channel(b)?,
            1.0,
        )),
        ("rgba", [r, g, b, a]) => Some(Color::rgba(
            parse_channel(r)?,
            parse_channel(g)?,
            parse_channel(b)?,
            a.trim()
                .parse::<f32>()
                .ok()
                .filter(|v| (0.0..=1.0).contains(v))?,
        )),
        _ => None,
    }
}

fn named_color(s: &str) -> Option<Color> {
    match s {
        "transparent" => Some(Color::TRANSPARENT),
        "black" => Some(Color::BLACK),
        "white" => Some(Color::WHITE),
        "red" => Some(Color::RED),
        "green" => Some(Color::GREEN),
        "blue" => Some(Color::BLUE),
        "gray" | "grey" => Some(Color::hex(0x808080)),
        "ink" => Some(Color::hex(0x0A0A0A)),
        "paper" => Some(Color::hex(0xF4F5F7)),
        "accent" => Some(Color::hex(0x1019EC)),
        _ => None,
    }
}

/// Parses any accepted color literal; `None` when nothing matches.
pub fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        return parse_hex_digits(hex);
    }
    if s.starts_with("rgb") {
        return parse_rgb_fn(s);
    }
    if let Some(c) = named_color(&s.to_lowercase()) {
        return Some(c);
    }
    // Bare hex without `#`: exactly 3/4/6/8 hex digits.
    if matches!(s.len(), 3 | 4 | 6 | 8) && s.bytes().all(|b| hex_val(b).is_some()) {
        return parse_hex_digits(s);
    }
    None
}

fn parse_px(s: &str) -> Option<f32> {
    s.strip_suffix("px")?.trim().parse::<f32>().ok()
}

/// Parses `fill` / `fit` / `fold` / `50%` / `12px` / `12`.
pub fn parse_size(s: &str) -> Option<Size> {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "fill" | "stretch" | "100%" => Some(Size::Fill),
        "fit" | "auto" => Some(Size::Fit),
        "fold" => Some(Size::Fold),
        _ => {
            if let Some(pct) = s.strip_suffix('%') {
                return pct
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .map(|v| Size::Percent(v / 100.0));
            }
            if let Some(px) = s.strip_suffix("px") {
                return px.trim().parse::<f32>().ok().map(Size::Fixed);
            }
            s.parse::<f32>().ok().map(Size::Fixed)
        }
    }
}

/// Parses `max` / `pill` / `12px` / `12`.
pub fn parse_radius(s: &str) -> Option<Radius> {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "max" | "pill" | "full" => Some(Radius::Max),
        _ => {
            let v = parse_px(&s).or_else(|| s.parse::<f32>().ok())?;
            Some(Radius::Scalar(v))
        }
    }
}

/// Parses `12px` / `12`. The legacy `full` keyword parses as zero.
pub fn parse_gap(s: &str) -> Option<Gap> {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "full" | "fill" => Some(Gap::Fixed(0.0)),
        _ => {
            let v = parse_px(&s).or_else(|| s.parse::<f32>().ok())?;
            Some(Gap::Fixed(v))
        }
    }
}

/// Parses CSS margin shorthand: `8` / `8 16` / `8 16 4` / `8 16 4 2`
/// (top / vertical horizontal / top horizontal bottom / clockwise).
/// Parts accept `px` suffixes.
pub fn parse_margin(s: &str) -> Option<Margin> {
    let parts: Vec<Option<f32>> = s
        .split_whitespace()
        .map(|p| parse_px(&p.to_lowercase()).or_else(|| p.parse::<f32>().ok()))
        .collect();
    if parts.is_empty() || parts.iter().any(|p| p.is_none()) {
        return None;
    }
    let v: Vec<f32> = parts.into_iter().flatten().collect();
    match v.as_slice() {
        [a] => Some(Margin::all(*a)),
        [y, x] => Some(Margin::symmetric(*y, *x)),
        [t, x, b] => Some(Margin {
            top: *t,
            right: *x,
            bottom: *b,
            left: *x,
        }),
        [t, r, b, l] => Some(Margin::sides(*t, *r, *b, *l)),
        _ => None,
    }
}

/// Parses CSS padding shorthand: `8` / `8 16` / `8 16 4` / `8 16 4 2`
/// (top / vertical horizontal / top horizontal bottom / clockwise).
/// Parts accept `px` suffixes.
pub fn parse_padding(s: &str) -> Option<Padding> {
    parse_margin(s).map(Padding::from)
}

pub fn parse_direction(s: &str) -> Option<Direction> {
    match s.trim().to_lowercase().as_str() {
        "horizontal" | "row" | "x" => Some(Direction::Horizontal),
        "vertical" | "column" | "col" | "y" => Some(Direction::Vertical),
        _ => None,
    }
}

pub fn parse_alignment(s: &str) -> Option<Alignment> {
    match s.trim().to_lowercase().as_str() {
        "start" | "flex-start" | "left" | "top" => Some(Alignment::Start),
        "center" | "middle" => Some(Alignment::Center),
        "end" | "flex-end" | "right" | "bottom" => Some(Alignment::End),
        "stretch" => Some(Alignment::Stretch),
        _ => None,
    }
}

pub fn parse_distribution(s: &str) -> Option<Distribution> {
    match s.trim().to_lowercase().as_str() {
        "start" | "flex-start" => Some(Distribution::Start),
        "center" => Some(Distribution::Center),
        "end" | "flex-end" => Some(Distribution::End),
        "space-between" | "between" => Some(Distribution::SpaceBetween),
        _ => None,
    }
}

/// Parses `wrap` / `nowrap` (`no-wrap`, `no_wrap`).
pub fn parse_flow_wrap(s: &str) -> Option<FlowWrap> {
    match s.trim().to_lowercase().replace(['-', '_'], "").as_str() {
        "wrap" => Some(FlowWrap::Wrap),
        "nowrap" | "no" | "none" => Some(FlowWrap::NoWrap),
        _ => None,
    }
}

/// Parses `wrap` / `nowrap` / `balance` (CSS `white-space` / `text-wrap` words).
pub fn parse_text_wrap(s: &str) -> Option<TextWrap> {
    match s.trim().to_lowercase().replace(['-', '_'], "").as_str() {
        "wrap" | "normal" | "prewrap" => Some(TextWrap::Wrap),
        "nowrap" | "no" | "none" => Some(TextWrap::NoWrap),
        "balance" => Some(TextWrap::Balance),
        _ => None,
    }
}

/// Parses `16/9`, `16:9`, `1.78`, or `16/9px` (suffix tolerated).
pub fn parse_aspect_ratio(s: &str) -> Option<AspectRatio> {
    let s = s.trim().to_lowercase();
    let s = s.strip_suffix("px").unwrap_or(&s);
    for sep in ['/', ':'] {
        if let Some((a, b)) = s.split_once(sep) {
            let w: f32 = a.trim().parse().ok()?;
            let h: f32 = b.trim().parse().ok()?;
            if h == 0.0 {
                return None;
            }
            return AspectRatio::new(w / h);
        }
    }
    AspectRatio::new(s.trim().parse().ok()?)
}

pub fn parse_stroke_align(s: &str) -> Option<StrokeAlign> {
    match s.trim().to_lowercase().as_str() {
        "inside" => Some(StrokeAlign::Inside),
        "center" => Some(StrokeAlign::Center),
        "outside" => Some(StrokeAlign::Outside),
        _ => None,
    }
}

pub fn parse_anchor(s: &str) -> Option<Anchor> {
    match s.trim().to_lowercase().replace(['-', '_'], "").as_str() {
        "topleft" | "tl" => Some(Anchor::TopLeft),
        "top" | "topcenter" | "tc" => Some(Anchor::Top),
        "topright" | "tr" => Some(Anchor::TopRight),
        "left" | "centerleft" => Some(Anchor::Left),
        "center" | "middle" => Some(Anchor::Center),
        "right" | "centerright" => Some(Anchor::Right),
        "bottomleft" | "bl" => Some(Anchor::BottomLeft),
        "bottom" | "bottomcenter" => Some(Anchor::Bottom),
        "bottomright" | "br" => Some(Anchor::BottomRight),
        _ => None,
    }
}

/// Parses `color` (1px inside) or `width color [inside|center|outside]`.
pub fn parse_stroke(s: &str) -> Option<Stroke> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    match parts.as_slice() {
        [color] => Some(Stroke::inside(1.0, parse_color(color)?)),
        [width, color] => Some(Stroke::inside(
            parse_px(&width.to_lowercase()).or_else(|| width.parse::<f32>().ok())?,
            parse_color(color)?,
        )),
        [width, color, align] => Some(Stroke::new(
            parse_px(&width.to_lowercase()).or_else(|| width.parse::<f32>().ok())?,
            Fill::solid(parse_color(color)?),
            parse_stroke_align(align)?,
        )),
        _ => None,
    }
}

/// Parses a bare number (elevation) or CSS-like `ox oy blur [spread] [color]`.
pub fn parse_shadow(s: &str) -> Option<Shadow> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    match parts.as_slice() {
        [single] => {
            let e = parse_px(&single.to_lowercase()).or_else(|| single.parse::<f32>().ok())?;
            Some(Shadow::elevation(e))
        }
        [ox, oy, blur] => Some(Shadow::web_like(
            num_part(ox)?,
            num_part(oy)?,
            num_part(blur)?,
            0.0,
            Color::BLACK,
        )),
        [ox, oy, blur, rest @ ..] => {
            // A trailing number is spread; anything else starts a color. A
            // lone non-number token is a color (`0 4 16 #0006`), not spread.
            let (spread, color) = match rest {
                [] => (0.0, Color::BLACK),
                [maybe_spread, color_parts @ ..] => match num_part(maybe_spread) {
                    Some(sp) if !color_parts.is_empty() => {
                        (sp, parse_color(&color_parts.join(" "))?)
                    }
                    Some(sp) if color_parts.is_empty() => (sp, Color::BLACK),
                    _ => (0.0, parse_color(&rest.join(" "))?),
                },
            };
            Some(Shadow::web_like(
                num_part(ox)?,
                num_part(oy)?,
                num_part(blur)?,
                spread,
                color,
            ))
        }
        [] => None,
        _ => None,
    }
}

fn num_part(s: &str) -> Option<f32> {
    parse_px(&s.to_lowercase()).or_else(|| s.parse::<f32>().ok())
}

/// Opaque magenta: the "this literal didn't parse" sentinel.
///
/// Rationale: an invalid literal is a programming error, but crashing the app
/// (or failing the build from inside a macro) would punish user components
/// that can't share primitive-only validation. So every literal-typed value
/// degrades to something loud-but-alive, identically for primitives and user
/// components. The `tracing::error!` (with value + grammar) is the dev-time
/// signal; tests assert the sentinels directly. No `debug_assert` panic here
/// by design: a typo must never unwind, in any profile.
pub const SENTINEL_COLOR: Color = Color::rgb(1.0, 0.0, 1.0);

fn lit_sentinel<T>(what: &str, got: &str, grammar: &str, fallback: T) -> T {
    tracing::error!("aurora: invalid {what} literal `{got}`; expected {grammar}; using fallback");
    fallback
}

macro_rules! impl_str_from {
    ($ty:ty, $parse:expr, $grammar:expr, $what:expr, $sentinel:expr) => {
        impl FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                $parse(s).ok_or_else(|| {
                    format!("invalid {} literal `{s}`; expected {}", $what, $grammar)
                })
            }
        }
        impl From<&str> for $ty {
            fn from(s: &str) -> Self {
                match $parse(s) {
                    Some(v) => v,
                    None => lit_sentinel($what, s, $grammar, $sentinel),
                }
            }
        }
        impl From<String> for $ty {
            fn from(s: String) -> Self {
                Self::from(s.as_str())
            }
        }
    };
}

impl_str_from!(Color, parse_color, COLOR_GRAMMAR, "color", SENTINEL_COLOR);
impl_str_from!(
    Fill,
    |s: &str| parse_color(s).map(Fill::solid),
    COLOR_GRAMMAR,
    "fill",
    Fill::Solid(SENTINEL_COLOR)
);
impl_str_from!(
    Size,
    parse_size,
    "`fill`, `fit`, `50%`, `12px`, or a number (px)",
    "size",
    Size::Fit
);
impl_str_from!(
    Radius,
    parse_radius,
    "`max`, `12px`, or a number (px)",
    "radius",
    Radius::ZERO
);
impl_str_from!(
    Gap,
    parse_gap,
    "`12px` or a number (px)",
    "gap",
    Gap::Fixed(0.0)
);
impl_str_from!(
    Margin,
    parse_margin,
    "1-4 numbers (`8`, `8 16`, `8 16 4`, `8 16 4 2`), `px` suffixes allowed",
    "margin",
    Margin::ZERO
);
impl_str_from!(
    Padding,
    parse_padding,
    "1-4 numbers (`8`, `8 16`, `8 16 4`, `8 16 4 2`), `px` suffixes allowed",
    "padding",
    Padding::ZERO
);
impl_str_from!(
    Direction,
    parse_direction,
    "`horizontal`/`row` or `vertical`/`column`",
    "direction",
    Direction::Horizontal
);
impl_str_from!(
    Alignment,
    parse_alignment,
    "`start`, `center`, `end`, or `stretch`",
    "alignment",
    Alignment::Start
);
impl_str_from!(
    Distribution,
    parse_distribution,
    "`start`, `center`, `end`, or `space-between`",
    "distribution",
    Distribution::Start
);
impl_str_from!(
    FlowWrap,
    parse_flow_wrap,
    "`wrap` or `nowrap`",
    "flow_wrap",
    FlowWrap::NoWrap
);
impl_str_from!(
    TextWrap,
    parse_text_wrap,
    "`wrap`, `nowrap`, or `balance`",
    "text_wrap",
    TextWrap::Wrap
);
impl_str_from!(
    AspectRatio,
    parse_aspect_ratio,
    "`16/9`, `16:9`, or a number",
    "aspect_ratio",
    AspectRatio::from(1.0)
);
impl_str_from!(
    Stroke,
    parse_stroke,
    "`color` or `width color [inside|center|outside]`",
    "stroke",
    Stroke::inside(1.0, SENTINEL_COLOR)
);
impl_str_from!(
    Anchor,
    parse_anchor,
    "`top-left`, `top`, `center`, `bottom-right`, … (dashes optional)",
    "anchor",
    Anchor::Center
);
impl_str_from!(
    Shadow,
    parse_shadow,
    "an elevation number or `offset-x offset-y blur [spread] [color]`",
    "shadow",
    Shadow::elevation(0.0)
);

impl From<u32> for Fill {
    fn from(hex: u32) -> Self {
        Self::solid(Color::hex(hex))
    }
}

impl From<i32> for Color {
    fn from(hex: i32) -> Self {
        Self::hex(hex as u32)
    }
}

impl From<i32> for Fill {
    fn from(hex: i32) -> Self {
        Self::solid(Color::hex(hex as u32))
    }
}

impl From<Color> for Fill {
    fn from(color: Color) -> Self {
        Self::solid(color)
    }
}

impl From<i32> for Size {
    fn from(value: i32) -> Self {
        Self::Fixed(value as f32)
    }
}

impl From<i32> for Radius {
    fn from(value: i32) -> Self {
        Self::Scalar(value as f32)
    }
}

impl From<u32> for Size {
    fn from(value: u32) -> Self {
        Self::Fixed(value as f32)
    }
}

impl From<u32> for Radius {
    fn from(value: u32) -> Self {
        Self::Scalar(value as f32)
    }
}

impl From<i32> for Gap {
    fn from(value: i32) -> Self {
        Self::Fixed(value as f32)
    }
}

impl From<u32> for Gap {
    fn from(value: u32) -> Self {
        Self::Fixed(value as f32)
    }
}

impl From<f32> for Margin {
    fn from(value: f32) -> Self {
        Self::all(value)
    }
}

impl From<i32> for Margin {
    fn from(value: i32) -> Self {
        Self::all(value as f32)
    }
}

impl From<u32> for Margin {
    fn from(value: u32) -> Self {
        Self::all(value as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(c: Color, r: f32, g: f32, b: f32, a: f32) {
        for (got, want) in [(c.r, r), (c.g, g), (c.b, b), (c.a, a)] {
            assert!((got - want).abs() < 0.01, "{c:?}");
        }
    }

    #[test]
    fn color_hex_forms() {
        approx(
            parse_color("#121212").unwrap(),
            0x12 as f32 / 255.0,
            0x12 as f32 / 255.0,
            0x12 as f32 / 255.0,
            1.0,
        );
        approx(parse_color("#fff").unwrap(), 1.0, 1.0, 1.0, 1.0);
        approx(
            parse_color("#0008").unwrap(),
            0.0,
            0.0,
            0.0,
            0x88 as f32 / 255.0,
        );
        approx(
            parse_color("121212").unwrap(),
            0x12 as f32 / 255.0,
            0x12 as f32 / 255.0,
            0x12 as f32 / 255.0,
            1.0,
        );
        assert!(parse_color("#xyz").is_none());
        assert!(parse_color("").is_none());
    }

    #[test]
    fn color_rgb_fn_and_names() {
        approx(
            parse_color("rgb(18, 18, 18)").unwrap(),
            18.0 / 255.0,
            18.0 / 255.0,
            18.0 / 255.0,
            1.0,
        );
        approx(
            parse_color("rgba(255, 0, 0, 0.5)").unwrap(),
            1.0,
            0.0,
            0.0,
            0.5,
        );
        assert_eq!(parse_color("red").unwrap(), Color::RED);
        assert_eq!(parse_color("transparent").unwrap(), Color::TRANSPARENT);
        assert!(parse_color("chartreuse-ish").is_none());
    }

    #[test]
    fn size_radius_gap_margin() {
        assert_eq!(parse_size("fill").unwrap(), Size::Fill);
        assert_eq!(parse_size("fit").unwrap(), Size::Fit);
        assert_eq!(parse_size("fold").unwrap(), Size::Fold);
        assert_eq!(parse_size("50%").unwrap(), Size::Percent(0.5));
        assert_eq!(parse_size("12px").unwrap(), Size::Fixed(12.0));
        assert_eq!(parse_size("12").unwrap(), Size::Fixed(12.0));
        assert_eq!(parse_radius("max").unwrap(), Radius::Max);
        assert_eq!(parse_radius("8").unwrap(), Radius::Scalar(8.0));
        assert_eq!(parse_gap("full").unwrap(), Gap::Fixed(0.0));
        assert_eq!(
            parse_gap("8").unwrap().main_px(),
            8.0
        );
        assert_eq!(parse_margin("8").unwrap(), Margin::all(8.0));
        assert_eq!(parse_margin("8 16").unwrap(), Margin::symmetric(8.0, 16.0));
        assert_eq!(
            parse_margin("1 2 3 4").unwrap(),
            Margin::sides(1.0, 2.0, 3.0, 4.0)
        );
        assert!(parse_margin("1 2 3 4 5").is_none());
    }

    #[test]
    fn enums_stroke_shadow() {
        assert_eq!(parse_direction("vertical").unwrap(), Direction::Vertical);
        assert_eq!(parse_direction("row").unwrap(), Direction::Horizontal);
        assert_eq!(parse_alignment("center").unwrap(), Alignment::Center);
        assert_eq!(
            parse_distribution("space-between").unwrap(),
            Distribution::SpaceBetween
        );
        assert_eq!(parse_anchor("bottom-right").unwrap(), Anchor::BottomRight);
        let st = parse_stroke("2 #fff center").unwrap();
        assert_eq!(st.width, 2.0);
        assert_eq!(parse_shadow("8").unwrap().elevation, 8.0);
        assert!(parse_shadow("nonsense here").is_none());
    }

    #[test]
    fn shadow_trailing_color_without_spread() {
        // Regression: `0 4 16 #0006` used to fail (lone trailing token was
        // forced into the spread slot); the color must win.
        let sh = parse_shadow("0 4 16 #0006").unwrap();
        assert_eq!(sh.elevation, 0.0);
        approx(sh.color, 0.0, 0.0, 0.0, 0x66 as f32 / 255.0);
        // Explicit spread still parses.
        let sh = parse_shadow("0 4 16 2 #fff").unwrap();
        assert_eq!(sh.spread, 2.0);
    }

    #[test]
    fn invalid_literals_degrade_to_sentinels_without_panicking() {
        // No-crash contract: typos render loudly and log, never unwind —
        // in any profile, so this test asserts the values directly.
        assert_eq!(Color::from("not-a-color"), SENTINEL_COLOR);
        assert_eq!(Size::from("huge"), Size::Fit);
        assert_eq!(Radius::from("round"), Radius::ZERO);
        assert_eq!(Margin::from("1 2 3 4 5"), Margin::ZERO);
        assert_eq!(Direction::from("diagonal"), Direction::Horizontal);
        assert_eq!(Shadow::from("nonsense here").elevation, 0.0);
    }
}
