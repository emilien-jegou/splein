// Responsibility: Validates ABI compatibility and resolves typed entry points from dynamic libraries.

use super::abi::{component_abi_fingerprint, host_sentinel_address};
use super::chrome::HotWindowInfo;
use super::dylib::DylibReloader;
use super::window::{ComponentRootFn, ComponentVersionFn, HotTitleFn, HotWindowFn};
use std::fmt;
use tracing::instrument;

/// Errors that can occur when resolving and validating UI entry points.
#[derive(Debug)]
pub enum ContractError {
    RuntimeMismatch { expected: usize, actual: usize },
    MissingSentinel,
    FingerprintMismatch { host: u64, dylib: u64 },
    MissingSymbol(&'static str),
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeMismatch { expected, actual } => write!(
                f,
                "hot-ui bound to a different runtime instance; expected {expected:#x}, got {actual:#x}"
            ),
            Self::MissingSentinel => write!(f, "runtime sentinel symbol missing"),
            Self::FingerprintMismatch { host, dylib } => write!(
                f,
                "ABI fingerprint mismatch; host={host:#x}, dylib={dylib:#x}"
            ),
            Self::MissingSymbol(sym) => write!(f, "missing required entry point: {sym}"),
        }
    }
}

impl std::error::Error for ContractError {}

/// Validated, type-safe entry points resolved from a fresh UI dynamic library.
pub struct LoadedUiContract {
    pub root: ComponentRootFn,
    pub window_info: HotWindowInfo,
    pub window_title: String,
}

impl LoadedUiContract {
    #[instrument(skip_all)]
    pub fn resolve(loader: &DylibReloader) -> Result<Self, ContractError> {
        let expected_sentinel = host_sentinel_address();
        match loader.bound_address(b"riode_runtime_sentinel") {
            Ok(actual) if actual == expected_sentinel => {}
            Ok(actual) => {
                return Err(ContractError::RuntimeMismatch {
                    expected: expected_sentinel,
                    actual,
                })
            }
            Err(_) => return Err(ContractError::MissingSentinel),
        }

        let expected_abi = component_abi_fingerprint();
        let version_fn: ComponentVersionFn = unsafe {
            *loader
                .get(b"riode_hot_version")
                .map_err(|_| ContractError::MissingSymbol("riode_hot_version"))?
        };

        let reported_abi = version_fn();
        if reported_abi != expected_abi {
            return Err(ContractError::FingerprintMismatch {
                host: expected_abi,
                dylib: reported_abi,
            });
        }

        let root: ComponentRootFn = unsafe {
            *loader
                .get(b"riode_component_root")
                .map_err(|_| ContractError::MissingSymbol("riode_component_root"))?
        };

        let window_info = unsafe {
            loader
                .get::<HotWindowFn>(b"riode_hot_window")
                .map(|f| f())
                .unwrap_or_else(|_| HotWindowInfo::fallback())
        };

        let window_title = unsafe {
            loader
                .get::<HotTitleFn>(b"riode_hot_title")
                .map(|f| f().to_string())
                .unwrap_or_else(|_| "Aurora Hot Reload".to_string())
        };

        Ok(Self {
            root,
            window_info,
            window_title,
        })
    }
}
