#![allow(missing_docs, unused, clippy::all)]

pub mod foundation;
pub mod materials;
pub mod shadow { pub use crate::materials::shadow::*; }
pub mod shadow_ext;
pub mod shadow_backend;

pub use foundation::*;
pub use materials::*;
pub use shadow::*;
pub use shadow_ext::*;
pub use shadow_backend::*;
pub use crate::shadow::api;
pub use crate::foundation::{BlendMode, FalloffCurve, LightEnvironment};

#[macro_export]
macro_rules! impl_str_from_shader {
    ($ty:ty, $parse:expr, $grammar:expr, $what:expr, $sentinel:expr) => {
        impl std::str::FromStr for $ty {
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
                    None => $sentinel,
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
