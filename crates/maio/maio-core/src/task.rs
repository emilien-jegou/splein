// Responsibility: Generic task models and serialization traits for macro-level asset storage.

//! Task representations and serialization contracts.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::{AssetId, MAIO_VERSION};

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("failed to encode task payload: {0}")]
    Encode(#[from] postcard::Error),
    #[error("failed to decode task payload: {0}")]
    Decode(postcard::Error),
}

/// Where the raw bytes of an asset originate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetSource {
    Path {
        path: PathBuf,
        content_hash: [u8; 16],
    },
    Inline(Vec<u8>),
}

impl AssetSource {
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Self::Path { path, .. } => Some(path),
            Self::Inline(_) => None,
        }
    }

    pub fn content_hash(&self) -> Option<&[u8; 16]> {
        match self {
            Self::Path { content_hash, .. } => Some(content_hash),
            Self::Inline(_) => None,
        }
    }
}

/// Trait implemented by domain-specific task parameters (e.g. ImageTask).
pub trait TaskSpec: Serialize + DeserializeOwned + Send + Sync + 'static {
    const KIND: &'static str;
}

/// Generic, domain-agnostic task recorded in .maiodb.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetTask {
    pub kind: String,
    pub source: AssetSource,
    pub payload: Vec<u8>,
    pub callsite: Option<String>,
}

impl AssetTask {
    /// Construct a task with an explicit source, typed parameters, and callsite metadata.
    pub fn new<T: TaskSpec>(source: AssetSource, spec: &T, callsite: Option<String>) -> Result<Self, TaskError> {
        let payload = postcard::to_stdvec(spec)?;
        Ok(Self {
            kind: T::KIND.to_string(),
            source,
            payload,
            callsite,
        })
    }

    pub fn to_spec<T: TaskSpec>(&self) -> Result<T, TaskError> {
        postcard::from_bytes(&self.payload).map_err(TaskError::Decode)
    }

    /// Content-addressed AssetId: Hash(content_hash + payload + engine_version).
    pub fn compute_id(&self, engine_version: &str) -> AssetId {
        let input = match &self.source {
            AssetSource::Path { content_hash, .. } => content_hash.as_slice(),
            AssetSource::Inline(bytes) => bytes.as_slice(),
        };
        AssetId::hash(input, &self.payload, engine_version)
    }

    pub fn id(&self) -> AssetId {
        self.compute_id(MAIO_VERSION)
    }

    pub fn source_path(&self) -> Option<&Path> {
        self.source.as_path()
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }
}
