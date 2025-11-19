use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

use crate::adep::AdepManifest;
use crate::hardware::scrubber::GpuScrubber;
use crate::runtime::{LaunchResult, RuntimeError};

/// Capsule represents a running capsule instance
#[derive(Clone, Debug)]
pub struct Capsule {
    pub id: String,
    pub adep_json: Vec<u8>,
    pub oci_image: String,
    pub digest: String,
    pub status: CapsuleStatus,
    pub storage_path: Option<String>,
    pub bundle_path: Option<String>,
    pub pid: Option<u32>,
    pub reserved_vram_bytes: u64,
    pub observed_vram_bytes: Option<u64>,
    pub last_failure: Option<String>,
    pub last_exit_code: Option<i32>,
    pub log_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CapsuleStatus {
    Pending,
    Running,
    Stopped,
    Failed,
}

impl ToString for CapsuleStatus {
    fn to_string(&self) -> String {
        match self {
            CapsuleStatus::Pending => "pending".to_string(),
            CapsuleStatus::Running => "running".to_string(),
            CapsuleStatus::Stopped => "stopped".to_string(),
            CapsuleStatus::Failed => "failed".to_string(),
        }
    }
}

/// CapsuleManager manages the lifecycle of capsules
pub struct CapsuleManager {
    capsules: Arc<RwLock<HashMap<String, Capsule>>>,
}

impl CapsuleManager {
    pub fn new() -> Self {
        Self {
            capsules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Deploy a new capsule
    pub async fn deploy_capsule(
        &self,
        capsule_id: String,
        adep_json: Vec<u8>,
        oci_image: String,
        digest: String,
    ) -> Result<String> {
        info!("Deploying capsule: {}", capsule_id);

        // Check if capsule already exists
        {
            let capsules = self
                .capsules
                .read()
                .map_err(|e| anyhow!("Lock error: {}", e))?;
            if capsules.contains_key(&capsule_id) {
                return Err(anyhow!("Capsule {} already exists", capsule_id));
            }
        }

        let capsule = Capsule {
            id: capsule_id.clone(),
            adep_json,
            oci_image,
            digest,
            status: CapsuleStatus::Pending,
            storage_path: None,
            bundle_path: None,
            pid: None,
            reserved_vram_bytes: 0,
            observed_vram_bytes: None,
            last_failure: None,
            last_exit_code: None,
            log_path: None,
        };

        // TODO: Actual deployment steps
        // 1. Validate manifest (already done by ValidateManifest RPC)
        // 2. Create storage (LVM/LUKS)
        // 3. Pull OCI image
        // 4. Create OCI bundle
        // 5. Start container (runc/youki)

        // For now, simulate deployment
        let mut deployed_capsule = capsule.clone();
        deployed_capsule.status = CapsuleStatus::Running;
        deployed_capsule.storage_path = Some(format!("/var/lib/capsuled/storage/{}", capsule_id));
        deployed_capsule.bundle_path = Some(format!("/var/lib/capsuled/bundles/{}", capsule_id));
        deployed_capsule.pid = Some(0);
        deployed_capsule.observed_vram_bytes = Some(0);

        // Store capsule
        {
            let mut capsules = self
                .capsules
                .write()
                .map_err(|e| anyhow!("Lock error: {}", e))?;
            capsules.insert(capsule_id.clone(), deployed_capsule);
        }

        info!("Capsule {} deployed successfully", capsule_id);
        Ok(CapsuleStatus::Running.to_string())
    }

    pub fn record_runtime_launch(
        &self,
        capsule_id: &str,
        manifest: &AdepManifest,
        manifest_json: &str,
        runtime: &LaunchResult,
        reserved_vram_bytes: u64,
    ) -> Result<()> {
        let mut capsules = self
            .capsules
            .write()
            .map_err(|e| anyhow!("Lock error: {}", e))?;

        let entry = capsules
            .entry(capsule_id.to_string())
            .or_insert_with(|| Capsule {
                id: capsule_id.to_string(),
                adep_json: Vec::new(),
                oci_image: manifest.compute.image.clone(),
                digest: manifest.metadata.get("digest").cloned().unwrap_or_default(),
                status: CapsuleStatus::Pending,
                storage_path: None,
                bundle_path: None,
                pid: None,
                reserved_vram_bytes: 0,
                observed_vram_bytes: None,
                last_failure: None,
                last_exit_code: None,
                log_path: None,
            });

        entry.adep_json = manifest_json.as_bytes().to_vec();
        entry.oci_image = manifest.compute.image.clone();
        entry.digest = manifest.metadata.get("digest").cloned().unwrap_or_default();
        entry.status = CapsuleStatus::Running;
        entry.storage_path = manifest.metadata.get("storage_path").cloned();
        entry.bundle_path = Some(runtime.bundle_path.to_string_lossy().to_string());
        entry.pid = Some(runtime.pid);
        entry.reserved_vram_bytes = reserved_vram_bytes;
        entry.observed_vram_bytes = None;
        entry.last_failure = None;
        entry.last_exit_code = None;
        entry.log_path = Some(runtime.log_path.to_string_lossy().to_string());

        info!(
            capsule_id = capsule_id,
            pid = runtime.pid,
            bundle = %runtime.bundle_path.display(),
            "Recorded capsule runtime launch"
        );

        Ok(())
    }

    pub fn record_runtime_failure(&self, capsule_id: &str, error: &RuntimeError) -> Result<()> {
        let mut capsules = self
            .capsules
            .write()
            .map_err(|e| anyhow!("Lock error: {}", e))?;

        let entry = capsules
            .entry(capsule_id.to_string())
            .or_insert_with(|| Capsule {
                id: capsule_id.to_string(),
                adep_json: Vec::new(),
                oci_image: String::new(),
                digest: String::new(),
                status: CapsuleStatus::Pending,
                storage_path: None,
                bundle_path: None,
                pid: None,
                reserved_vram_bytes: 0,
                observed_vram_bytes: None,
                last_failure: None,
                last_exit_code: None,
                log_path: None,
            });

        entry.status = CapsuleStatus::Failed;
        entry.pid = None;
        entry.observed_vram_bytes = None;
        entry.last_failure = Some(error.to_string());
        entry.last_exit_code = match error {
            RuntimeError::CommandFailure { exit_code, .. } => *exit_code,
            _ => None,
        };

        warn!(
            capsule_id = capsule_id,
            "Recorded runtime failure: {}",
            entry
                .last_failure
                .as_deref()
                .unwrap_or("unknown runtime error")
        );

        Ok(())
    }

    /// Stop and remove a capsule
    pub async fn stop_capsule(&self, capsule_id: &str) -> Result<()> {
        info!("Stopping capsule: {}", capsule_id);

        // 1. Update status to Stopped (to prevent new requests)
        {
            let mut capsules = self
                .capsules
                .write()
                .map_err(|e| anyhow!("Lock error: {}", e))?;
            if let Some(capsule) = capsules.get_mut(capsule_id) {
                capsule.status = CapsuleStatus::Stopped;
            } else {
                return Err(anyhow!("Capsule {} not found", capsule_id));
            }
        }

        // TODO: Call OCI runtime to kill/delete container
        // self.runtime.kill(capsule_id, "SIGKILL").await?;
        // self.runtime.delete(capsule_id).await?;

        // 2. VRAM Scrubbing (Post-Stop Hook)
        // TODO: Get assigned GPU index from capsule state
        // For now, we assume single GPU (index 0) if this is a GPU workload
        // In Phase 2, we will look up the actual assigned GPU ID.
        let assigned_gpu_index = 0; // Mock: Always scrub GPU 0

        info!("Executing VRAM Scrubbing for GPU {}...", assigned_gpu_index);
        match GpuScrubber::scrub_device(assigned_gpu_index) {
            Ok(_) => info!("VRAM Scrubbing successful for capsule {}", capsule_id),
            Err(e) => {
                // In production, this should be a critical alert
                warn!("VRAM Scrubbing failed: {:?}. Security risk!", e);
                // Don't return error here to allow cleanup to proceed, but log heavily
            }
        }

        // 3. Remove from manager
        {
            let mut capsules = self
                .capsules
                .write()
                .map_err(|e| anyhow!("Lock error: {}", e))?;
            capsules.remove(capsule_id);
        }

        info!("Capsule {} stopped and removed", capsule_id);
        Ok(())
    }

    /// Get a capsule by ID
    pub fn get_capsule(&self, capsule_id: &str) -> Result<Capsule> {
        let capsules = self
            .capsules
            .read()
            .map_err(|e| anyhow!("Lock error: {}", e))?;
        capsules
            .get(capsule_id)
            .cloned()
            .ok_or_else(|| anyhow!("Capsule {} not found", capsule_id))
    }

    /// List all capsules
    pub fn list_capsules(&self) -> Result<Vec<Capsule>> {
        let capsules = self
            .capsules
            .read()
            .map_err(|e| anyhow!("Lock error: {}", e))?;
        Ok(capsules.values().cloned().collect())
    }

    /// Get count of capsules by status
    pub fn count_by_status(&self, status: CapsuleStatus) -> Result<usize> {
        let capsules = self
            .capsules
            .read()
            .map_err(|e| anyhow!("Lock error: {}", e))?;
        Ok(capsules.values().filter(|c| c.status == status).count())
    }

    /// Update the observed VRAM usage for a capsule based on runtime measurements.
    pub fn update_observed_vram(&self, capsule_id: &str, observed_vram_bytes: u64) -> Result<()> {
        let mut capsules = self
            .capsules
            .write()
            .map_err(|e| anyhow!("Lock error: {}", e))?;

        match capsules.get_mut(capsule_id) {
            Some(capsule) => {
                capsule.observed_vram_bytes = Some(observed_vram_bytes);
                debug!(
                    capsule_id = capsule_id,
                    observed_vram_bytes, "Updated observed VRAM for capsule"
                );
                Ok(())
            }
            None => Err(anyhow!("Capsule {} not found", capsule_id)),
        }
    }
}

impl Default for CapsuleManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_deploy_capsule() {
        let manager = CapsuleManager::new();
        let result = manager
            .deploy_capsule(
                "test-capsule".to_string(),
                b"{\"name\":\"test\",\"version\":\"1.0\"}".to_vec(),
                "alpine:latest".to_string(),
                "sha256:abc123".to_string(),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "running");

        // Verify capsule exists
        let capsule = manager.get_capsule("test-capsule").unwrap();
        assert_eq!(capsule.id, "test-capsule");
        assert_eq!(capsule.status, CapsuleStatus::Running);
        assert_eq!(capsule.pid, Some(0));
        assert_eq!(capsule.reserved_vram_bytes, 0);
        assert!(capsule.last_failure.is_none());
    }

    #[tokio::test]
    async fn test_stop_capsule() {
        let manager = CapsuleManager::new();

        // Deploy first
        manager
            .deploy_capsule(
                "test-capsule".to_string(),
                b"{}".to_vec(),
                "alpine:latest".to_string(),
                "sha256:abc123".to_string(),
            )
            .await
            .unwrap();

        // Stop
        let result = manager.stop_capsule("test-capsule").await;
        assert!(result.is_ok());

        // Verify capsule is removed
        let capsule = manager.get_capsule("test-capsule");
        assert!(capsule.is_err());
    }

    #[tokio::test]
    async fn test_capsule_not_found() {
        let manager = CapsuleManager::new();
        let result = manager.stop_capsule("non-existent").await;
        assert!(result.is_err());
    }
}
