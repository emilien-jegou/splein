use std::str::FromStr;

use crate::foundation::alignment::{Alignment, Direction, Distribution, FlowWrap};
use crate::foundation::color::Color;
use crate::foundation::fill::Fill;
use crate::foundation::filter::Filter;
use crate::foundation::positioning::Anchor;
use crate::foundation::sizing::{AspectRatio, Size};
use crate::foundation::spacing::{Gap, Margin, Padding};
use crate::text::TextWrap;

pub const COLOR_GRAMMAR: &str =
    "`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, or a name";

pub const SENTINEL_COLOR: Color = Color::rgb(1.0, 0.0, 1.0);

fn lit_sentinel<T>(_what: &str, _got: &str, _grammar: &str, fallback: T) -> T {
    fallback
}

#[macro_export]
macro_rules! impl_str_from {
    ($ty:ty, $parse:expr, $grammar:expr, $what:expr, $sentinel:expr) => {
        impl std::str::FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                $parse(s).ok_or_else(|| format!("invalid {} literal `{s}`; expected {}", $what, $grammar))
            }
        }
        impl From<&str> for $ty {
            fn from(s: &str) -> Self {
                match $parse(s) {
                    Some(v) => v,
                    None => $crate::foundation::literal::lit_sentinel($what, s, $grammar, $sentinel),
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

pub use crate::impl_str_from;
