// Responsibility: maio-core domain models, determinism, and content-addressed hashing.

//! Core types and content-addressing logic for the maio asset engine.

#![warn(missing_docs)]

pub mod task;

pub use task::{AssetSource, AssetTask, TaskError, TaskSpec};

use blake3::Hasher;
use serde::{Deserialize, Serialize};

/// Engine version baked into every [`AssetId`] and artifact header.
pub const MAIO_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Content-addressed asset identifier: 16-byte cryptographic hash.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AssetId([u8; 16]);

impl AssetId {
    /// Hash raw input bytes and transform parameters into a stable 16-byte identity.
    pub fn hash(input: &[u8], transform_params: &[u8], engine_version: &str) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(input);
        hasher.update(&[0x00]);
        hasher.update(transform_params);
        hasher.update(&[0x01]);
        hasher.update(engine_version.as_bytes());
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
        Self::from_bytes(bytes)
    }

    /// Construct an AssetId from raw bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Access the underlying 16-byte hash slice.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Full 32-character lowercase hex string.
    pub fn as_hex(&self) -> String {
        hex(&self.0)
    }

    /// First 8 characters of hex identity for diagnostic logs.
    pub fn short_hex(&self) -> String {
        self.as_hex()[..8].to_owned()
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_hex())
    }
}

impl std::fmt::Debug for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AssetId").field(&self.as_hex()).finish()
    }
}

/// Error returned when parsing an AssetId from a hex string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid AssetId: expected 32 hex characters, got {0} characters")]
pub struct AssetIdParseError(usize);

impl std::str::FromStr for AssetId {
    type Err = AssetIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();
        if bytes.len() != 32 {
            return Err(AssetIdParseError(bytes.len()));
        }
        let mut raw = [0u8; 16];
        for (i, chunk) in bytes.chunks_exact(2).enumerate() {
            raw[i] = (hex_val(chunk[0]).ok_or(AssetIdParseError(32))? << 4)
                | hex_val(chunk[1]).ok_or(AssetIdParseError(32))?;
        }
        Ok(Self::from_bytes(raw))
    }
}

const fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    let table = b"0123456789abcdef";
    for &b in bytes {
        out.push(table[(b >> 4) as usize] as char);
        out.push(table[(b & 0xf) as usize] as char);
    }
    out
}
