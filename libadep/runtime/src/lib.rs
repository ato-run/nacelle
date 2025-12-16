//! ADEP container runtime abstraction layer
//!
//! This library provides a platform-agnostic interface for running ADEP capsules.
//! It abstracts the differences between container runtimes on different platforms:
//! - Linux: youki (OCI-compliant container runtime)
//! - Windows/macOS: simple process execution
//!
//! # Example
//!
//! ```no_run
//! use libadep_runtime::{create_runtime, AdepContainerRuntime, CapsuleManifest};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let runtime = create_runtime();
//!     let manifest = CapsuleManifest::load(Path::new("adep.json"))?;
//!     let capsule_root = Path::new("/path/to/capsule");
//!
//!     let capsule_id = runtime.run(&manifest, capsule_root).await?;
//!     println!("Started capsule: {}", capsule_id);
//!
//!     Ok(())
//! }
//! ```

pub mod error;
pub mod manifest;
pub mod runtimes;

use error::Result;
pub use manifest::CapsuleManifest;
pub use runtimes::{SimpleProcessRuntime, YoukiRuntime};
use std::path::Path;

/// Creates the optimal runtime for the current platform
///
/// - Linux: Returns `YoukiRuntime` (OCI-compliant container runtime)
/// - Windows/macOS: Returns `SimpleProcessRuntime` (direct process execution)
pub fn create_runtime() -> Box<dyn AdepContainerRuntime + Send + Sync> {
    if cfg!(target_os = "linux") {
        Box::new(YoukiRuntime::new())
    } else {
        Box::new(SimpleProcessRuntime::new())
    }
}

/// Abstract interface for ADEP container runtimes
///
/// This trait defines the operations that all container runtime implementations
/// must support, regardless of the underlying execution mechanism.
#[async_trait::async_trait]
pub trait AdepContainerRuntime {
    /// Executes a capsule based on the provided manifest
    ///
    /// # Arguments
    ///
    /// * `manifest` - The capsule manifest containing runtime configuration
    /// * `capsule_root` - The directory containing the capsule's files and adep.json
    ///
    /// # Returns
    ///
    /// A unique capsule ID that can be used for subsequent operations (stop, list)
    async fn run(&self, manifest: &CapsuleManifest, capsule_root: &Path) -> Result<String>;

    /// Stops a running capsule
    ///
    /// # Arguments
    ///
    /// * `capsule_id` - The unique ID returned by `run()`
    async fn stop(&self, capsule_id: &str) -> Result<()>;

    /// Lists all running capsules
    ///
    /// # Returns
    ///
    /// A vector of capsule IDs for all currently running capsules
    async fn list(&self) -> Result<Vec<String>>;
}
