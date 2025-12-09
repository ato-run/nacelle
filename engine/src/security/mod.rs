//! Security module for Capsule signing and verification

pub mod signing;

pub use signing::{CapsuleSigner, CapsuleSignature, CapsuleVerifier, TrustedKeyStore};

pub mod audit;

pub use audit::{AuditLogger, AuditOperation, AuditStatus};

pub mod egress_proxy;

pub use egress_proxy::EgressProxy;

pub mod path;
pub mod vram_scrubber;

pub use path::validate_path;
