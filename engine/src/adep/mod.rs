//! Adep (Application Definition and Execution Protocol) module
//!
//! Re-exports CapsuleManifestV1 types from libadep-core.
//! This replaces the legacy AdepManifest structure.

pub use libadep_core::capsule_v1::{
    CapsuleManifestV1, CapsuleRequirements, CapsuleExecution, CapsuleStorage, StorageVolume,
    RuntimeType, Platform, CapsuleType,
};
