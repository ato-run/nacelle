pub mod index;
pub mod safety;
pub mod storage;
pub mod verify;

pub use index::{
    CanonicalIndex, CompressedEntry, Duplicate, DuplicateKind, IndexChange, IndexDiff, IndexEntry,
    IndexMetadata, IndexUpdate, MergeConflict, MergeConflictKind, MergeReport,
};
pub use storage::{BlobStatus, BlobStore, IngestOptions, StoredBlob};
pub use verify::{CompressedHash, VerificationResult, Verifier};

#[derive(thiserror::Error, Debug)]
pub enum CasError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid index format: {0}")]
    InvalidIndex(String),
    #[error("Hash mismatch")]
    HashMismatch,
    #[error("Compressed hash mismatch for algorithm: {0}")]
    CompressedHashMismatch(String),
    #[error("Unsupported compression algorithm: {0}")]
    UnsupportedCompression(String),
    #[error("Decompression failed: {0}")]
    Decompression(String),
    #[error(
        "compression ratio exceeded: raw {raw} bytes vs compressed {compressed} bytes (limit {limit}x)"
    )]
    CompressionRatioExceeded {
        raw: u64,
        compressed: u64,
        limit: f64,
    },
    #[error("archive entry escapes target directory: {entry}")]
    ZipSlip { entry: String },
}
