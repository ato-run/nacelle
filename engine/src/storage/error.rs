use thiserror::Error;

/// Storage operation errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Volume not found: {0}")]
    VolumeNotFound(String),

    #[error("Volume already exists: {0}")]
    VolumeAlreadyExists(String),

    #[error("Invalid volume name: {0}")]
    InvalidVolumeName(String),

    #[error("Invalid size specification: {0}")]
    InvalidSize(String),

    #[error("Encryption error: {0}")]
    EncryptionError(String),

    #[error("Key management error: {0}")]
    KeyManagementError(String),

    #[error("Insufficient space: required {required}, available {available}")]
    InsufficientSpace { required: u64, available: u64 },

    #[error("Snapshot error: {0}")]
    SnapshotError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Volume is busy: {0}")]
    VolumeBusy(String),
}

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;
