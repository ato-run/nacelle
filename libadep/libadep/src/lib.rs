#![cfg_attr(not(feature = "std"), no_std)]

pub use libadep_cas as cas;
pub use libadep_core as core;
// Re-export capsule manifest types for convenience
pub use libadep_core::capsule_manifest;
pub use libadep_deps as deps;
pub use libadep_observability as observability;

// Runtime module for executing capsules locally
// Re-export the main runtime API for convenience
#[cfg(feature = "std")]
pub use libadep_runtime as runtime;

#[cfg(feature = "std")]
pub use libadep_runtime::{
    create_runtime, AdepContainerRuntime, CapsuleManifest, SimpleProcessRuntime, YoukiRuntime,
};
