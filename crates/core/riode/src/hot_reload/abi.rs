// Responsibility: Computes deterministic ABI fingerprints and validates runtime sentinels across dylib boundaries.

use crate::dsl::element::Element;
use std::mem::{align_of, size_of};

/// Manual protocol generation bumped on semantic breaking changes.
const COMPONENT_PROTO_GEN: u64 = 1;

/// FNV-1a offset basis.
pub(crate) const FNV_OFFSET: u64 = 0xcbf29ce484222325;

/// FNV-1a over one `u64`.
pub(crate) fn fold_u64(hash: u64, value: u64) -> u64 {
    let mut h = hash;
    for b in value.to_le_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// FNV-1a over a byte slice.
pub(crate) fn fold_bytes(hash: u64, bytes: &[u8]) -> u64 {
    let mut h = hash;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Toolchain identity reported by the compiler during build.
pub fn toolchain_id() -> &'static str {
    option_env!("RIODE_TOOLCHAIN_ID")
        .or(option_env!("AURORA_TOOLCHAIN_ID"))
        .unwrap_or("unknown-toolchain")
}

/// Computes the machine-checked ABI fingerprint required for code-swap handshakes.
pub fn component_abi_fingerprint() -> u64 {
    let mut h = FNV_OFFSET;
    h = fold_u64(h, COMPONENT_PROTO_GEN);
    h = fold_u64(h, size_of::<Element>() as u64);
    h = fold_u64(h, align_of::<Element>() as u64);
    h = fold_bytes(h, toolchain_id().as_bytes());
    h
}

/// Dynamic linker scope sentinel symbol.
#[no_mangle]
pub extern "Rust" fn riode_runtime_sentinel() {}

/// Returns the host's memory address for the runtime sentinel.
pub fn host_sentinel_address() -> usize {
    riode_runtime_sentinel as *const () as usize
}
