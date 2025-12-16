//! Youki-based OCI container runtime for Linux
//!
//! This module provides integration with youki, a container runtime written in Rust
//! that implements the OCI (Open Container Initiative) runtime specification.
//!
//! # Implementation Status
//!
//! **Current**: Stub implementation (not yet functional)
//! **Planned**: Phase 2 - Full OCI container support for Linux environments
//!
//! # Future Design
//!
//! The production implementation will:
//! - Generate OCI-compliant config.json from CapsuleManifest
//! - Create container rootfs with proper isolation
//! - Use youki to spawn isolated container processes
//! - Manage container lifecycle (create, start, stop, delete)
//! - Implement network namespace and port mapping

use crate::{AdepContainerRuntime, CapsuleManifest, Result};
use std::path::Path;

/// Youki-based container runtime (Linux only)
///
/// This runtime will use the youki container runtime to execute capsules
/// in isolated OCI-compliant containers with proper resource limits and
/// security boundaries.
///
/// # Note
///
/// This is currently a stub implementation. Actual OCI container support
/// will be added in Phase 2 of the Gumball project.
pub struct YoukiRuntime {
    // Future: Store youki binary path, container state directory, etc.
}

impl YoukiRuntime {
    /// Creates a new YoukiRuntime instance
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for YoukiRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AdepContainerRuntime for YoukiRuntime {
    async fn run(&self, _manifest: &CapsuleManifest, _capsule_root: &Path) -> Result<String> {
        // TODO: Phase 2 implementation
        // 1. Generate OCI config.json from manifest
        // 2. Prepare container rootfs
        // 3. Call youki create <container-id>
        // 4. Call youki start <container-id>
        // 5. Return container ID

        anyhow::bail!(
            "YoukiRuntime is not yet implemented. \
            Use SimpleProcessRuntime for development, or wait for Phase 2 OCI support."
        )
    }

    async fn stop(&self, _capsule_id: &str) -> Result<()> {
        // TODO: Phase 2 implementation
        // 1. Call youki kill <container-id>
        // 2. Call youki delete <container-id>
        // 3. Clean up container state

        anyhow::bail!(
            "YoukiRuntime is not yet implemented. \
            Use SimpleProcessRuntime for development, or wait for Phase 2 OCI support."
        )
    }

    async fn list(&self) -> Result<Vec<String>> {
        // TODO: Phase 2 implementation
        // Query youki state directory for running containers

        anyhow::bail!(
            "YoukiRuntime is not yet implemented. \
            Use SimpleProcessRuntime for development, or wait for Phase 2 OCI support."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Entrypoint;

    #[tokio::test]
    async fn test_youki_runtime_not_implemented() {
        let runtime = YoukiRuntime::new();
        let manifest = CapsuleManifest {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            entrypoint: Entrypoint {
                command: "node".to_string(),
                args: vec!["index.js".to_string()],
            },
            network: None,
        };

        let result = runtime.run(&manifest, Path::new(".")).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not yet implemented"));
    }
}
