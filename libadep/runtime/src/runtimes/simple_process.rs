//! Simple process-based runtime for Windows and macOS
//!
//! This runtime executes capsules as regular OS processes without containerization.
//! It's designed for development environments and platforms without native container support.

use crate::{AdepContainerRuntime, CapsuleManifest, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Simple process-based runtime implementation
///
/// This runtime manages capsules as child processes, tracking them in a shared
/// HashMap protected by a Mutex. Each capsule is assigned a unique UUID that
/// can be used for lifecycle management (stop, list).
///
/// # Example
///
/// ```no_run
/// use libadep_runtime::{SimpleProcessRuntime, AdepContainerRuntime, CapsuleManifest};
/// use std::path::Path;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let runtime = SimpleProcessRuntime::new();
///     let manifest = CapsuleManifest::load(Path::new("adep.json"))?;
///     let capsule_root = Path::new("/path/to/capsule");
///
///     let capsule_id = runtime.run(&manifest, capsule_root).await?;
///     println!("Capsule running with ID: {}", capsule_id);
///
///     // Later...
///     runtime.stop(&capsule_id).await?;
///     Ok(())
/// }
/// ```
pub struct SimpleProcessRuntime {
    /// Active child processes indexed by capsule ID
    active_processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl SimpleProcessRuntime {
    /// Creates a new SimpleProcessRuntime instance
    pub fn new() -> Self {
        Self {
            active_processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for SimpleProcessRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AdepContainerRuntime for SimpleProcessRuntime {
    async fn run(&self, manifest: &CapsuleManifest, capsule_root: &Path) -> Result<String> {
        // Generate unique capsule ID
        let capsule_id = uuid::Uuid::new_v4().to_string();

        // Build command from manifest
        let mut command = Command::new(&manifest.entrypoint.command);
        command.args(&manifest.entrypoint.args);

        //作業ディレクトリを `rootfs` に設定
        let working_dir = capsule_root.join(".webcapsule/rootfs");
        command.current_dir(working_dir);

        // Ensure child process is killed when the parent (Tauri app) terminates
        command.kill_on_drop(true);

        // Spawn the child process
        let child = command.spawn().map_err(|e| {
            anyhow::anyhow!(
                "Failed to spawn capsule process '{}': {}",
                manifest.entrypoint.command,
                e
            )
        })?;

        // Store the child process for later management
        self.active_processes
            .lock()
            .await
            .insert(capsule_id.clone(), child);

        Ok(capsule_id)
    }

    async fn stop(&self, capsule_id: &str) -> Result<()> {
        let mut processes = self.active_processes.lock().await;

        if let Some(mut child) = processes.remove(capsule_id) {
            // Attempt to kill the process
            child
                .kill()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to kill capsule {}: {}", capsule_id, e))?;
        }
        // If capsule_id not found, silently succeed (idempotent operation)

        Ok(())
    }

    async fn list(&self) -> Result<Vec<String>> {
        let processes = self.active_processes.lock().await;
        Ok(processes.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Entrypoint, NetworkConfig, PortMapping};
    use std::env;

    fn create_test_manifest() -> CapsuleManifest {
        CapsuleManifest {
            name: "test-capsule".to_string(),
            version: "1.0.0".to_string(),
            entrypoint: Entrypoint {
                command: "echo".to_string(),
                args: vec!["hello".to_string()],
            },
            network: Some(NetworkConfig {
                ports: vec![PortMapping {
                    container_port: 3000,
                    host_port: 3000,
                }],
            }),
        }
    }

    #[tokio::test]
    async fn test_simple_process_runtime_new() {
        let runtime = SimpleProcessRuntime::new();
        let capsules = runtime.list().await.unwrap();
        assert_eq!(capsules.len(), 0);
    }

    #[tokio::test]
    async fn test_run_and_list() {
        let runtime = SimpleProcessRuntime::new();
        let manifest = create_test_manifest();
        let capsule_root = env::current_dir().unwrap();

        let capsule_id = runtime.run(&manifest, &capsule_root).await.unwrap();
        assert!(!capsule_id.is_empty());

        let capsules = runtime.list().await.unwrap();
        assert_eq!(capsules.len(), 1);
        assert!(capsules.contains(&capsule_id));
    }

    #[tokio::test]
    async fn test_run_and_stop() {
        let runtime = SimpleProcessRuntime::new();
        let manifest = create_test_manifest();
        let capsule_root = env::current_dir().unwrap();

        let capsule_id = runtime.run(&manifest, &capsule_root).await.unwrap();

        // Stop the capsule
        runtime.stop(&capsule_id).await.unwrap();

        // Verify it's removed from the list
        let capsules = runtime.list().await.unwrap();
        assert_eq!(capsules.len(), 0);
    }

    #[tokio::test]
    async fn test_stop_nonexistent_capsule() {
        let runtime = SimpleProcessRuntime::new();
        // Stopping a non-existent capsule should succeed (idempotent)
        let result = runtime.stop("non-existent-id").await;
        assert!(result.is_ok());
    }
}
