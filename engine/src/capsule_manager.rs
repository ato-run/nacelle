use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

use crate::adep::AdepManifest;
use crate::hardware::scrubber::GpuScrubber;
use crate::hardware::GpuDetector;
use crate::runtime::{LaunchResult, RuntimeError};
use crate::security::audit::{AuditLogger, AuditOperation, AuditStatus};

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

use crate::network::mdns::MdnsAnnouncer;
use crate::network::service_registry::ServiceRegistry;
use crate::network::traefik::TraefikManager;

/// Manages the lifecycle of capsules
pub struct CapsuleManager {
    capsules: RwLock<HashMap<String, Capsule>>,
    audit_logger: Arc<AuditLogger>,
    gpu_scrubber: Arc<GpuScrubber>,
    gpu_detector: Arc<dyn GpuDetector>,
    service_registry: Option<Arc<ServiceRegistry>>,
    mdns_announcer: Option<Arc<MdnsAnnouncer>>,
    traefik_manager: Option<Arc<TraefikManager>>,
    cloud_client: Option<Arc<crate::cloud::skypilot::SkyPilotClient>>,
}

impl CapsuleManager {
    pub fn new(
        audit_logger: Arc<AuditLogger>,
        gpu_scrubber: Arc<GpuScrubber>,
        gpu_detector: Arc<dyn GpuDetector>,
        service_registry: Option<Arc<ServiceRegistry>>,
        mdns_announcer: Option<Arc<MdnsAnnouncer>>,
        traefik_manager: Option<Arc<TraefikManager>>,
        cloud_client: Option<Arc<crate::cloud::skypilot::SkyPilotClient>>,
    ) -> Self {
        Self {
            capsules: RwLock::new(HashMap::new()),
            audit_logger,
            gpu_scrubber,
            gpu_detector,
            service_registry,
            mdns_announcer,
            traefik_manager,
            cloud_client,
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
        info!("Deploying capsule {}", capsule_id);

        // Parse manifest to check resources
        let manifest: AdepManifest = serde_json::from_slice(&adep_json)
            .map_err(|e| anyhow!("Failed to parse adep_json: {}", e))?;

        // Placement Logic
        let mut assigned_gpu_index = None;

        if manifest.requires_gpu() {
            let required_vram = manifest.required_vram_bytes();
            info!("Capsule requires GPU with {} bytes VRAM", required_vram);

            // 1. Local Scan
            let report = self.gpu_detector.detect_gpus()
                .map_err(|e| anyhow!("Failed to detect GPUs: {}", e))?;
            
            let mut found_local = false;
            for gpu in report.gpus {
                match self.gpu_detector.get_available_vram_bytes(gpu.index as usize) {
                    Ok(available) => {
                        if available >= required_vram {
                            info!("Found suitable local GPU {} (Available: {} bytes)", gpu.index, available);
                            assigned_gpu_index = Some(gpu.index);
                            found_local = true;
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get VRAM for GPU {}: {}", gpu.index, e);
                    }
                }
            }

            // 2. Decision
            if !found_local {
                info!("Local resources insufficient (Required: {} bytes). Checking cloud options...", required_vram);
                
                if let Some(cloud_config) = &manifest.scheduling.cloud {
                    if cloud_config.accelerators.is_some() {
                        info!("Cloud accelerators requested. Bursting to cloud...");
                        if let Some(client) = &self.cloud_client {
                            // TODO: Pass full manifest or specific cloud request
                            // For now, we just pass the raw JSON string as a placeholder
                            let manifest_str = std::str::from_utf8(&adep_json).unwrap_or("");
                            return client.deploy(manifest_str).await
                                .map_err(|e| anyhow!("Cloud deployment failed: {}", e));
                        } else {
                            return Err(anyhow!("Cloud bursting required but Cloud Client is not configured"));
                        }
                    }
                }
                
                return Err(anyhow!("Insufficient resources: No local GPU with enough VRAM and no cloud configuration found."));
            } else {
                info!("Deploying locally to GPU {:?}", assigned_gpu_index);
            }
        }

        // 1. Allocate Port (if registry available)
        let port = if let Some(registry) = &self.service_registry {
            match registry.allocate_port() {
                Some(p) => {
                    info!("Allocated port {} for capsule {}", p, capsule_id);
                    Some(p)
                }
                None => {
                    warn!("Failed to allocate port for capsule {}", capsule_id);
                    None
                }
            }
        } else {
            None
        };

        // 2. Start Container (Mock logic for now)
        // In a real implementation, we would pass the port to the container runtime
        // and wait for it to be ready.
        // For verification purposes, we spawn a simple Python HTTP server to bind the port.
        let pid = if let Some(p) = port {
            use std::process::{Command, Stdio};
            
            // Determine directory to serve
            // We look for examples/apps/<capsule_id> relative to the project root
            // Assuming we are running from capsuled/engine or root
            let mut serve_dir = std::env::current_dir().unwrap_or_default();
            
            // Try to find the project root by looking for "examples"
            let candidate_paths = vec![
                "examples/apps",
                "../examples/apps",
                "../../examples/apps",
            ];
            
            let mut app_dir = None;
            for path in candidate_paths {
                let candidate = serve_dir.join(path).join(&capsule_id);
                if candidate.exists() && candidate.join("index.html").exists() {
                    app_dir = Some(candidate);
                    break;
                }
            }
            
            let (working_dir, is_temp) = if let Some(dir) = app_dir {
                info!("Serving custom app UI from {:?}", dir);
                (dir, false)
            } else {
                // Create a temporary directory with a default index.html
                let temp_dir = std::env::temp_dir().join("capsuled_mock_apps").join(&capsule_id);
                if let Err(e) = std::fs::create_dir_all(&temp_dir) {
                    warn!("Failed to create temp dir: {}", e);
                    (serve_dir, false)
                } else {
                    let index_html = format!(
                        "<html><body><h1>{}</h1><p>No custom UI found in examples/apps/{}.</p></body></html>",
                        capsule_id, capsule_id
                    );
                    if let Err(e) = std::fs::write(temp_dir.join("index.html"), index_html) {
                        warn!("Failed to write default index.html: {}", e);
                        (serve_dir, false)
                    } else {
                        info!("Serving default UI from {:?}", temp_dir);
                        (temp_dir, true)
                    }
                }
            };

            info!("Spawning mock runtime (python3 http.server) on port {}", p);
            match Command::new("python3")
                .arg("-m")
                .arg("http.server")
                .arg(p.to_string())
                .current_dir(working_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    info!("Mock runtime started with PID {}", child.id());
                    Some(child.id())
                }
                Err(e) => {
                    warn!("Failed to spawn mock runtime: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // 3. Register Service
        if let (Some(registry), Some(p)) = (&self.service_registry, port) {
            registry.register_service(capsule_id.clone(), p, vec!["http".to_string()]);

            // 4. Broadcast via mDNS
            if let Some(mdns) = &self.mdns_announcer {
                if let Err(e) = mdns.register(&capsule_id, p) {
                    warn!("Failed to register mDNS for {}: {}", capsule_id, e);
                }
            }

            // 5. Update Traefik Routes
            if let Some(traefik) = &self.traefik_manager {
                let services = registry.get_services();
                if let Err(e) = traefik.update_routes(&services) {
                    warn!("Failed to update Traefik routes: {}", e);
                }
            }
        }

        // Log audit event
        if let Err(e) = self.audit_logger.log_event(
            &AuditOperation::DeployCapsule.to_string(),
            None,
            &AuditStatus::Success.to_string(),
            Some(format!("Deployed capsule {}", capsule_id)),
        ) {
            error!("Failed to log audit event: {}", e);
        }

        {
            let mut capsules = self
                .capsules
                .write()
                .map_err(|e| anyhow!("Lock error: {}", e))?;
            capsules.insert(
                capsule_id.clone(),
                Capsule {
                    id: capsule_id.clone(),
                    adep_json,
                    oci_image,
                    digest,
                    status: CapsuleStatus::Running,
                    storage_path: None,
                    bundle_path: None,
                    pid: pid.or(Some(0)), // Use real PID or fallback to 0
                    reserved_vram_bytes: 0,
                    observed_vram_bytes: None,
                    last_failure: None,
                    last_exit_code: None,
                    log_path: None,
                },
            );
        }

        Ok("running".to_string())
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

        // Audit Log: CAPSULE_DEPLOY
        self.audit_logger.log_event(
            &AuditOperation::DeployCapsule.to_string(),
            None,
            &AuditStatus::Success.to_string(),
            Some(format!(
                "Deployed workload {} (image={})",
                capsule_id, manifest.compute.image
            )),
        )?;

        // Audit Log: CAPSULE_START
        self.audit_logger.log_event(
            &AuditOperation::StartCapsule.to_string(),
            None,
            &AuditStatus::Success.to_string(),
            Some(format!(
                "Started workload {} (pid={})",
                capsule_id, runtime.pid
            )),
        )?;

        Ok(())
    }

    pub fn record_runtime_start(&self, capsule_id: &str) -> Result<()> {
        self.audit_logger.log_event(
            &AuditOperation::StartCapsule.to_string(),
            None,
            &AuditStatus::Success.to_string(),
            Some(format!("Started capsule {}", capsule_id)),
        )?;
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

    /// Update observed VRAM usage for a capsule
    pub fn update_observed_vram(&self, capsule_id: &str, observed_bytes: u64) -> Result<()> {
        let mut capsules = self
            .capsules
            .write()
            .map_err(|e| anyhow!("Lock error: {}", e))?;

        if let Some(capsule) = capsules.get_mut(capsule_id) {
            capsule.observed_vram_bytes = Some(observed_bytes);
            Ok(())
        } else {
            Err(anyhow!("Capsule {} not found", capsule_id))
        }
    }

    pub fn list_capsules(&self) -> Result<Vec<Capsule>> {
        let capsules = self
            .capsules
            .read()
            .map_err(|e| anyhow!("Lock error: {}", e))?;
        Ok(capsules.values().cloned().collect())
    }

    /// Stop and remove a capsule
    pub async fn stop_capsule(&self, capsule_id: &str) -> Result<()> {
        info!("Stopping capsule {}", capsule_id);

        // 1. Stop Container (Mock logic)
        {
            let capsules = self.capsules.read().map_err(|e| anyhow!("Lock error: {}", e))?;
            if let Some(capsule) = capsules.get(capsule_id) {
                if let Some(pid) = capsule.pid {
                    if pid > 0 {
                        info!("Killing mock runtime process PID {}", pid);
                        // Use libc or std::process::Command to kill
                        // Since we don't have the Child handle stored, we use kill command
                        let _ = std::process::Command::new("kill")
                            .arg(pid.to_string())
                            .output();
                    }
                }
            }
        }

        // 2. Unregister Service
        if let Some(registry) = &self.service_registry {
            registry.unregister_service(capsule_id);

            // 3. Stop mDNS Broadcast
            if let Some(mdns) = &self.mdns_announcer {
                if let Err(e) = mdns.unregister(capsule_id) {
                    warn!("Failed to unregister mDNS for {}: {}", capsule_id, e);
                }
            }

            // 4. Update Traefik Routes
            if let Some(traefik) = &self.traefik_manager {
                let services = registry.get_services();
                if let Err(e) = traefik.update_routes(&services) {
                    warn!("Failed to update Traefik routes: {}", e);
                }
            }
        }

        // Log audit event
        if let Err(e) = self.audit_logger.log_event(
            &AuditOperation::StopCapsule.to_string(),
            None,
            &AuditStatus::Success.to_string(),
            Some(format!("Stopped capsule {}", capsule_id)),
        ) {
            error!("Failed to log audit event: {}", e);
        }

        {
            let mut capsules = self
                .capsules
                .write()
                .map_err(|e| anyhow!("Lock error: {}", e))?;
            if capsules.remove(capsule_id).is_none() {
                return Err(anyhow!("Capsule {} not found", capsule_id));
            }
        }

        // Scrub GPU memory
        if let Err(e) = self.gpu_scrubber.scrub_all_gpus().await {
            error!("Failed to scrub GPUs after capsule stop: {}", e);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_manager() -> CapsuleManager {
        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("audit.log");
        let key_path = temp_dir.path().join("node_key.pem");

        let logger =
            Arc::new(AuditLogger::new(log_path, key_path, "test-node".to_string()).unwrap());
        let scrubber = Arc::new(GpuScrubber::new(logger.clone()));
        let gpu_detector = crate::hardware::create_gpu_detector();

        CapsuleManager::new(logger, scrubber, gpu_detector, None, None, None, None)
    }

    #[tokio::test]
    async fn test_deploy_capsule() {
        let manager = create_test_manager();
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
        // Note: get_capsule returns Capsule struct which is different from CapsuleStatus
        // We need to adjust this test or the get_capsule method if we want to test internal state
    }

    #[tokio::test]
    async fn test_stop_capsule() {
        let manager = create_test_manager();

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
    }

    #[tokio::test]
    async fn test_capsule_not_found() {
        let manager = create_test_manager();
        let result = manager.stop_capsule("non-existent").await;
        assert!(result.is_err());
    }
}
