// Single responsibility: Rendering backend failure type shared by all rasterizers.

use std::fmt;

/// Failure raised while initializing, rasterizing, or presenting a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    /// Backend or device construction failed.
    Init(String),
    /// Rasterization of display list commands failed.
    Raster(String),
    /// OS surface acquisition or presentation failed.
    Surface(String),
}

impl BackendError {
    /// Constructs an initialization failure.
    pub fn init(message: impl Into<String>) -> Self {
        Self::Init(message.into())
    }

    /// Constructs a rasterization failure.
    pub fn raster(message: impl Into<String>) -> Self {
        Self::Raster(message.into())
    }

    /// Constructs a surface presentation failure.
    pub fn surface(message: impl Into<String>) -> Self {
        Self::Surface(message.into())
    }
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init(message) => write!(f, "backend init failed: {message}"),
            Self::Raster(message) => write!(f, "rasterization failed: {message}"),
            Self::Surface(message) => write!(f, "surface present failed: {message}"),
        }
    }
}

impl std::error::Error for BackendError {}
