use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{error, info, warn};

use crate::adep::AdepManifest;
use crate::hardware::GpuDetector;
use crate::runtime::{
    ContainerRuntime, DevRuntime, LaunchRequest, LaunchResult, NativeRuntime, Runtime,
    RuntimeConfig, RuntimeError,
};
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

    pub started_at: Option<std::time::SystemTime>,
    pub remote_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CapsuleStatus {
    Pending,
    Provisioning,
    Running,
    Stopped,
    Failed,
}

impl std::fmt::Display for CapsuleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapsuleStatus::Pending => write!(f, "pending"),
            CapsuleStatus::Provisioning => write!(f, "provisioning"),
            CapsuleStatus::Running => write!(f, "running"),
            CapsuleStatus::Stopped => write!(f, "stopped"),
            CapsuleStatus::Failed => write!(f, "failed"),
        }
    }
}

use crate::artifact::ArtifactManager;
use crate::network::mdns::MdnsAnnouncer;
use crate::network::service_registry::ServiceRegistry;
use crate::network::traefik::TraefikManager;
use crate::process_supervisor::ProcessSupervisor;

/// Manages the lifecycle of capsules
pub struct CapsuleManager {
    capsules: RwLock<HashMap<String, Capsule>>,
    audit_logger: Arc<AuditLogger>,
    gpu_detector: Arc<dyn GpuDetector>,
    service_registry: Option<Arc<ServiceRegistry>>,
    mdns_announcer: Option<Arc<MdnsAnnouncer>>,
    traefik_manager: Option<Arc<TraefikManager>>,
    cloud_client: Option<Arc<crate::cloud::skypilot::SkyPilotClient>>,
    
    // Runtimes
    container_runtime: Arc<ContainerRuntime>,
    native_runtime: Arc<NativeRuntime>,
    dev_runtime: Arc<DevRuntime>,
    
    // Artifact Manager
    artifact_manager: Option<Arc<ArtifactManager>>,
}

impl CapsuleManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        audit_logger: Arc<AuditLogger>,
        gpu_detector: Arc<dyn GpuDetector>,
        service_registry: Option<Arc<ServiceRegistry>>,
        mdns_announcer: Option<Arc<MdnsAnnouncer>>,
        traefik_manager: Option<Arc<TraefikManager>>,
        cloud_client: Option<Arc<crate::cloud::skypilot::SkyPilotClient>>,
        artifact_manager: Option<Arc<ArtifactManager>>,
        process_supervisor: Option<Arc<ProcessSupervisor>>,
        egress_proxy_port: Option<u16>,
        runtime_config: Option<RuntimeConfig>,
    ) -> Result<Self, crate::runtime::RuntimeError> {
        // Initialize runtimes
        let runtime_config = if let Some(config) = runtime_config {
            config
        } else {
            // TODO: Load config from file or env
            match RuntimeConfig::from_section(None) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to detect container runtime: {}. Container workloads will fail.", e);
                    // Fallback to dummy config to allow Engine to start (e.g. for NativeRuntime usage)
                    RuntimeConfig {
                        kind: crate::runtime::RuntimeKind::Mock,
                        binary_path: std::path::PathBuf::from("/dev/null"),
                        bundle_root: std::env::temp_dir().join("capsuled").join("bundles"),
                        state_root: std::env::temp_dir().join("capsuled").join("state"),
                        log_dir: std::env::temp_dir().join("capsuled").join("logs"),
                        hook_retry_attempts: 1,
                    }
                }
            }
        };

        let container_runtime = Arc::new(ContainerRuntime::new(runtime_config));
        let native_runtime = Arc::new(NativeRuntime::new(
            artifact_manager.clone(),
            process_supervisor.clone(),
            egress_proxy_port,
        ));
        let dev_runtime = Arc::new(DevRuntime::new(
            process_supervisor.clone(),
            egress_proxy_port,
        ));

        Ok(Self {
            capsules: RwLock::new(HashMap::new()),
            audit_logger,
            gpu_detector,
            service_registry,
            mdns_announcer,
            traefik_manager,
            cloud_client,
            container_runtime,
            native_runtime,
            dev_runtime,
            artifact_manager,
        })
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
        
        // Manifest JSON string for passing to runtimes
        let manifest_json_str = std::str::from_utf8(&adep_json).unwrap_or("{}");

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
                            // 1. Register Capsule as Provisioning
                            let capsule = Capsule {
                                id: capsule_id.clone(),
                                adep_json: adep_json.clone(),
                                oci_image: oci_image.clone(),
                                digest: digest.clone(),
                                status: CapsuleStatus::Provisioning,
                                storage_path: None,
                                bundle_path: None,
                                pid: None,
                                reserved_vram_bytes: 0, // Cloud VRAM
                                observed_vram_bytes: None,
                                last_failure: None,
                                last_exit_code: None,
                                log_path: None,
                                started_at: Some(std::time::SystemTime::now()),
                                remote_url: None,
                            };
                            self.capsules.write().unwrap().insert(capsule_id.clone(), capsule);

                            // 2. Deploy
                            match client.deploy(manifest_json_str).await {
                                Ok(cluster_name) => {
                                    // 3. Update to Running with URL
                                    let url = format!("https://{}.ts.net", cluster_name);
                                    
                                    if let Some(c) = self.capsules.write().unwrap().get_mut(&capsule_id) {
                                        c.status = CapsuleStatus::Running;
                                        c.remote_url = Some(url.clone());
                                    }
                                    
                                    return Ok(format!("Cloud deployment started: {}", url));
                                }
                                Err(e) => {
                                    // Update to Failed
                                    if let Some(c) = self.capsules.write().unwrap().get_mut(&capsule_id) {
                                        c.status = CapsuleStatus::Failed;
                                        c.last_failure = Some(e.to_string());
                                    }
                                    return Err(anyhow!("Cloud deployment failed: {}", e));
                                }
                            }
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

        // Ensure Runtime Artifact (if native)
        if let Some(native_config) = &manifest.compute.native {
            if let Some(am) = &self.artifact_manager {
                let (runtime_id, version) = if let Some(at_pos) = native_config.runtime.find('@') {
                    let (id, ver) = native_config.runtime.split_at(at_pos);
                    (id, &ver[1..])
                } else {
                    (native_config.runtime.as_str(), "latest")
                };

                info!("Ensuring runtime artifact {}@{}...", runtime_id, version);
                match am.ensure_runtime(runtime_id, version, None).await {
                    Ok(path) => {
                        info!("Runtime artifact ready at {:?}", path);
                    }
                    Err(e) => {
                        warn!("Failed to ensure runtime artifact: {}. Deployment may fail if not cached.", e);
                        // We could return error here, but NativeRuntime might handle it or use direct path.
                        // But if ArtifactManager is configured, we expect it to work.
                        return Err(anyhow!("Failed to ensure runtime artifact: {}", e));
                    }
                }
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

        // 2. Select Runtime and Launch
        // Prepare ComputeConfig with injected env vars
        let mut compute_config = manifest.compute.clone();
        
        if let Some(p) = port {
            compute_config.env.push(format!("PORT={}", p));
        }
        
        // GPU UUIDs for build_oci_spec
        let gpu_uuids = if let Some(gpu_idx) = assigned_gpu_index {
             // We only have index here, but build_oci_spec expects UUIDs?
             // Or does it handle indices?
             // The GpuDetector returns GpuInfo which has index.
             // We should probably get UUID from detector if possible, or just pass index as string if that's what we have.
             // But wait, `build_oci_spec` expects `Option<&[String]>` (UUIDs).
             // `GpuDetector` trait has `detect_gpus` returning `GpuReport` with `GpuInfo`.
             // `GpuInfo` has `uuid` field? Let's check `hardware/mod.rs` or `gpu_detector.rs`.
             // Assuming we can get UUID or just use index for now.
             // If we only have index, we might need to look up UUID.
             // For now let's pass index as string if we can't get UUID easily, 
             // BUT `build_oci_spec` puts it in `NVIDIA_VISIBLE_DEVICES`.
             // NVIDIA_VISIBLE_DEVICES supports indices too.
             Some(vec![gpu_idx.to_string()])
        } else {
             None
        };

        // Build the spec
        let allowed_paths = vec![]; 
        // We need a rootfs path. For now use a dummy or configured one.
        // `ContainerRuntime` prepares bundle, but `build_oci_spec` needs `rootfs_path` to build `root` config.
        // `ContainerRuntime` uses `bundle_root`.
        // We should probably use a placeholder here and let `ContainerRuntime` fix it?
        // Or `ContainerRuntime` should build the spec?
        // `ContainerRuntime::prepare_bundle` writes `config.json`.
        // `LaunchRequest` passes `Spec`.
        // So we need to build `Spec` here.
        // We need a valid rootfs path for the spec.
        // Let's use `rootfs` from config or temp.
        let rootfs_path = std::path::PathBuf::from("/"); // Placeholder

        let spec = crate::oci::spec_builder::build_oci_spec(
            &rootfs_path,
            &compute_config,
            &manifest.volumes,
            gpu_uuids.as_deref(),
            &allowed_paths,
            None, // resources
        ).map_err(|e| anyhow!("Failed to build OCI spec: {}", e))?;

        let launch_request = LaunchRequest {
            workload_id: &capsule_id,
            spec: &spec,
            manifest_json: Some(manifest_json_str),
        };

        // Determine which runtime to use
        let runtime: Arc<dyn Runtime> = if manifest.compute.native.is_some() {
            // Check if we are on macOS
            if cfg!(target_os = "macos") {
                self.native_runtime.clone()
            } else {
                // Fallback to DevRuntime (Mock) if not macOS? 
                // Or error? Original logic fell back to Mock if not native compatible?
                // Original logic: if native config exists AND target_os=macos -> Native.
                // Else -> Mock.
                self.dev_runtime.clone()
            }
        } else if !oci_image.is_empty() {
             // If OCI image is specified, use ContainerRuntime
             self.container_runtime.clone()
        } else {
             // Fallback to DevRuntime (Mock)
             self.dev_runtime.clone()
        };

        let launch_result = match runtime.launch(launch_request).await {
            Ok(res) => res,
            Err(e) => {
                self.record_runtime_failure(&capsule_id, &e)?;
                return Err(anyhow!("Runtime launch failed: {}", e));
            }
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

        // Record success
        self.record_runtime_launch(
            &capsule_id,
            &manifest,
            manifest_json_str,
            &launch_result,
            0, // TODO: Track reserved VRAM
        )?;

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
                started_at: None,
                remote_url: None,
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
        entry.started_at = Some(std::time::SystemTime::now());

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
                started_at: None,
                remote_url: None,
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

    pub fn get_capsule_pid(&self, capsule_id: &str) -> Option<u32> {
        let capsules = self.capsules.read().ok()?;
        capsules.get(capsule_id).and_then(|c| c.pid)
    }

    pub fn get_capsule_log_path(&self, capsule_id: &str) -> Option<String> {
        let capsules = self.capsules.read().ok()?;
        capsules.get(capsule_id).and_then(|c| c.log_path.clone())
    }

    /// Stop and remove a capsule
    pub async fn stop_capsule(&self, capsule_id: &str) -> Result<()> {
        info!("Stopping capsule {}", capsule_id);

        let adep_json = {
            let capsules = self.capsules.read().map_err(|e| anyhow!("Lock error: {}", e))?;
            capsules.get(capsule_id).map(|c| c.adep_json.clone())
        };

        if let Some(json) = adep_json {
             let manifest: AdepManifest = serde_json::from_slice(&json).unwrap_or_default();
             
             let runtime: Arc<dyn Runtime> = if manifest.compute.native.is_some() {
                if cfg!(target_os = "macos") {
                    self.native_runtime.clone()
                } else {
                    self.dev_runtime.clone()
                }
            } else if !manifest.compute.image.is_empty() {
                 self.container_runtime.clone()
            } else {
                 self.dev_runtime.clone()
            };

            if let Err(e) = runtime.stop(capsule_id).await {
                warn!("Failed to stop runtime for {}: {}", capsule_id, e);
            }
        } else {
            warn!("Capsule {} not found or has no manifest, cannot stop runtime", capsule_id);
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
            
            if let Some(capsule) = capsules.get_mut(capsule_id) {
                capsule.status = CapsuleStatus::Stopped;
                capsule.pid = None;
                // Keep log_path and other info
            } else {
                return Err(anyhow!("Capsule {} not found", capsule_id));
            }
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
        // Keep temp_dir alive? It will be dropped at end of function, but paths are used.
        // In tests, temp_dir is usually dropped at end of test.
        // Here we return Manager which holds paths.
        // We should leak temp_dir or recreate it in test?
        // Actually, `tempfile::tempdir()` creates a directory that is deleted on Drop.
        // If we drop `temp_dir` here, the directory is gone.
        // We should probably just use a static path or leak it for tests.
        // Or better, make `create_test_manager` return `(CapsuleManager, TempDir)`.
        // But for now let's just leak it to avoid complexity    fn create_test_manager() -> CapsuleManager {
        let temp_dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        
        // Create mock runtime script
        let mock_runtime_path = temp_dir.path().join("mock_runtime");
        let script = r#"#!/bin/sh
case "$1" in
    state)
        echo '{"pid": 1234, "status": "running"}'
        ;;
    create|start|delete|kill)
        exit 0
        ;;
    *)
        exit 0
        ;;
esac
"#;
        std::fs::write(&mock_runtime_path, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&mock_runtime_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let log_path = temp_dir.path().join("audit.log");
        let key_path = temp_dir.path().join("node_key.pem");

        let logger =
            Arc::new(AuditLogger::new(log_path, key_path, "test-node".to_string()).unwrap());
        let gpu_detector = crate::hardware::create_gpu_detector();

        let runtime_config = crate::runtime::RuntimeConfig {
            kind: crate::runtime::RuntimeKind::Mock,
            binary_path: mock_runtime_path,
            bundle_root: temp_dir.path().join("bundles"),
            state_root: temp_dir.path().join("state"),
            log_dir: temp_dir.path().join("logs"),
            hook_retry_attempts: 1,
        };

        CapsuleManager::new(
            logger, 
            gpu_detector, 
            None, 
            None, 
            None, 
            None, 
            None, 
            None, 
            None, 
            Some(runtime_config)
        ).unwrap()
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
