//! Adep (Application Definition and Execution Protocol) module
//!
//! Re-exports CapsuleManifestV1 types from libadep-core.
//! This replaces the legacy AdepManifest structure.

pub use libadep_core::capsule_v1::{
    CapsuleExecution, CapsuleManifestV1, CapsuleRequirements, CapsuleStorage, CapsuleType,
    Platform, RuntimeType, StorageVolume,
};
