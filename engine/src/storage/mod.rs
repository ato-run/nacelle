/// Storage management module for LVM and LUKS
///
/// This module provides abstractions for:
/// - LVM (Logical Volume Manager) volume management
/// - LUKS (Linux Unified Key Setup) encryption
///
/// All implementations are pure Rust with no CGO dependencies.
pub mod error;
pub mod lvm;
pub mod luks;

pub use error::{StorageError, StorageResult};
pub use lvm::{LvmManager, VolumeInfo};
pub use luks::{EncryptedVolumeInfo, KeyStorage, LuksManager};
