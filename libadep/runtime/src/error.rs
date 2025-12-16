//! Error types for libadep-runtime

pub use anyhow::Error;

/// Result type alias for runtime operations
pub type Result<T> = std::result::Result<T, Error>;
