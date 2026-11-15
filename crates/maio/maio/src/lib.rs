// Responsibility: The .maiodb journal: multi-process ACID storage error types and public re-exports.

mod journal;
mod store;

pub use journal::{ArtifactMeta, Record};
pub use store::{Maiodb, Snapshot, MANIFEST};

use std::fmt;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum MaioError {
    #[error("Manifest not found at `{}`. No assets registered yet (run your build to initialize the manifest).", path.display())]
    ManifestNotFound { path: PathBuf },
    #[error("io error on {context}: {source}")]
    Io {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to acquire file lock: {0}")]
    Lock(String),
    #[error("corrupt journal frame at offset {offset}: {reason}")]
    Corrupt {
        offset: u64,
        reason: String,
    },
    #[error("journal record encoding error: {0}")]
    Encoding(#[from] postcard::Error),
    #[error("unknown asset id {0}")]
    UnknownId(String),
}

impl From<std::io::Error> for MaioError {
    fn from(source: std::io::Error) -> Self {
        Self::Io { context: "filesystem", source }
    }
}

pub type Result<T, E = MaioError> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TornTail {
    pub offset: u64,
}

impl fmt::Display for TornTail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "torn tail at byte {}", self.offset)
    }
}
