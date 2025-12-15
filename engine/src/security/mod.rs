//! Security module for Capsule signing and verification

pub mod signing;

pub use signing::{CapsuleSigner, CapsuleSignature, CapsuleVerifier, TrustedKeyStore};

pub mod audit;

pub use audit::{AuditLogger, AuditOperation, AuditStatus};

pub mod egress_proxy;

pub use egress_proxy::EgressProxy;

pub mod egress_policy;

pub use egress_policy::{EgressPolicyRegistry, ENV_KEY_EGRESS_TOKEN, META_KEY_EGRESS_ALLOWLIST};

pub mod path;
pub mod vram_scrubber;

pub use path::validate_path;
