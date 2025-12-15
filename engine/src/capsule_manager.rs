use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{info, warn};

use crate::adep::AdepManifest;
#[cfg(feature = "toml-support")]
use libadep_core::capsule_v1::CapsuleManifestV1;
use crate::hardware::GpuDetector;
use crate::runtime::{
    ContainerRuntime, DevRuntime, DockerCliRuntime, LaunchRequest, LaunchResult, NativeRuntime, Runtime,
    RuntimeConfig, RuntimeError,
};
use crate::security::audit::{AuditLogger, AuditOperation, AuditStatus};
use crate::security::vram_scrubber::VramScrubber;
use crate::billing::usage::UsageTracker;
use crate::billing::reporter::UsageReporter;
use crate::storage::{StorageConfig, StorageManager};

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
    pub user_id: Option<String>,
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
    docker_cli_runtime: Arc<DockerCliRuntime>,
    
    // Artifact Manager
    artifact_manager: Option<Arc<ArtifactManager>>,

    // Storage Manager (Phase 5)
    storage_manager: Option<Arc<StorageManager>>,

    // Billing
    usage_tracker: Arc<UsageTracker>,
    usage_reporter: Option<Arc<UsageReporter>>,
}

impl CapsuleManager {

    #[cfg(feature = "toml-support")]
    fn enforce_manifest_signature(&self, adep_json: &[u8]) -> Result<()> {
        let manifest_str = std::str::from_utf8(adep_json)
            .map_err(|e| anyhow!("Manifest is not valid UTF-8: {}", e))?;

        if let Ok(canonical) = CapsuleManifestV1::from_toml(manifest_str) {
            canonical
                .validate()
                .map_err(|e| anyhow!("Manifest validation failed: {}", e))?;
            return canonical
                .verify_signature()
                .map_err(|e| anyhow!("Manifest signature verification failed: {}", e));
        }

        if let Ok(canonical) = serde_json::from_str::<CapsuleManifestV1>(manifest_str) {
            canonical
                .validate()
                .map_err(|e| anyhow!("Manifest validation failed: {}", e))?;
            return canonical
                .verify_signature()
                .map_err(|e| anyhow!("Manifest signature verification failed: {}", e));
        }

        // If the payload looks like a capsule_v1 manifest but failed to parse, fail closed.
        if manifest_str.contains("schema_version") {
            return Err(anyhow!(
                "Failed to parse capsule_v1 manifest for signature verification"
            ));
        }

        Ok(())
    }

    #[cfg(not(feature = "toml-support"))]
    fn enforce_manifest_signature(&self, _adep_json: &[u8]) -> Result<()> {
        Ok(())
    }
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
        usage_reporter: Option<Arc<UsageReporter>>,
        storage_config: Option<StorageConfig>,
    ) -> Result<Self, crate::runtime::RuntimeError> {
        // Initialize runtimes
        let runtime_config = if let Some(config) = runtime_config {
            config
        } else {
            // TODO: Load config from file or env
            match RuntimeConfig::from_section(None) {
                Ok(c) => c,
                Err(e) => {
                    warn!(
                        "External OCI runtime binary not detected: {}. Falling back to Mock ContainerRuntime (NativeRuntime may still work)",
                        e
                    );
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

        let container_runtime = Arc::new(ContainerRuntime::new(
            runtime_config,
            artifact_manager.clone(),
            process_supervisor.clone(),
            egress_proxy_port,
        ));
        let native_runtime = Arc::new(NativeRuntime::new(
            artifact_manager.clone(),
            process_supervisor.clone(),
            egress_proxy_port,
        ));
        let dev_runtime = Arc::new(DevRuntime::new(
            process_supervisor.clone(),
            egress_proxy_port,
        ));
        let docker_cli_runtime = Arc::new(DockerCliRuntime::new(egress_proxy_port));

        // Initialize StorageManager if config provided
        let storage_manager = storage_config.map(|config| {
            info!("Initializing StorageManager with VG: {}", config.default_vg);
            Arc::new(StorageManager::new(config))
        });

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
            docker_cli_runtime,
            artifact_manager,
            storage_manager,
            usage_tracker: Arc::new(UsageTracker::new()),
            usage_reporter,
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

        // Enforce libadep signature verification before proceeding
        self.enforce_manifest_signature(&adep_json)
            .map_err(|e| {
                warn!("Manifest signature verification failed for {}: {}", capsule_id, e);
                e
            })?;

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
                                user_id: None,
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
            let runtime_id = native_config.runtime.as_str();
            
            // Skip artifact download for system-level runtimes (mlx, llama, vllm)
            // These are expected to be installed via pip/homebrew and accessed directly
            let system_runtimes = ["mlx", "llama", "vllm"];
            let is_system_runtime = system_runtimes.iter().any(|r| runtime_id.starts_with(r));
            
            if is_system_runtime {
                info!("Using system runtime '{}' - skipping artifact download", runtime_id);
            } else if let Some(am) = &self.artifact_manager {
                let (runtime_id, version) = if let Some(at_pos) = native_config.runtime.find('@') {
                    let (id, ver) = native_config.runtime.split_at(at_pos);
                    (id, &ver[1..])
                } else {
                    (runtime_id, "latest")
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

        // 1. Determine Port
        // If the caller (e.g. Coordinator) already injected PORT, honor it so the returned
        // URL maps to a real listener on the host.
        let desired_port: Option<u16> = manifest
            .compute
            .env
            .iter()
            .find_map(|e| {
                let (k, v) = e.split_once('=')?;
                if k == "PORT" {
                    v.parse::<u16>().ok()
                } else {
                    None
                }
            });

        let port = if let Some(p) = desired_port {
            info!("Using pre-injected PORT {} for capsule {}", p, capsule_id);
            Some(p)
        } else if let Some(registry) = &self.service_registry {
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
            let has_port_env = compute_config
                .env
                .iter()
                .any(|e| e.starts_with("PORT="));
            if !has_port_env {
                compute_config.env.push(format!("PORT={}", p));
            }
        }

        // Egress policy: if the manifest declares an allowlist, mint a token and
        // register it with the global proxy policy registry.
        if let Some(value) = manifest
            .metadata
            .get(crate::security::META_KEY_EGRESS_ALLOWLIST)
        {
            let allowlist = crate::security::egress_policy::parse_allowlist_csv(value);
            if !allowlist.is_empty() {
                let token = uuid::Uuid::new_v4().to_string();
                crate::security::EgressPolicyRegistry::global().register(
                    &capsule_id,
                    token.clone(),
                    allowlist,
                );

                let token_kv = format!("{}={}", crate::security::ENV_KEY_EGRESS_TOKEN, token);
                if !compute_config.env.iter().any(|e| e.starts_with(crate::security::ENV_KEY_EGRESS_TOKEN)) {
                    compute_config.env.push(token_kv);
                }
            }
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
        // Resolve rootfs path.
        // - Prefer explicit rootfs hint from manifest metadata (if present)
        // - Then CAPSULED_DEFAULT_ROOTFS env
        // - Finally, create a minimal temp rootfs (satisfies ContainerRuntime's existence check)
        let rootfs_path = if let Some(rootfs) = manifest.metadata.get("rootfs_path") {
            std::path::PathBuf::from(rootfs)
        } else if let Ok(rootfs) = std::env::var("CAPSULED_DEFAULT_ROOTFS") {
            std::path::PathBuf::from(rootfs)
        } else {
            std::env::temp_dir().join("capsuled").join("rootfs")
        };

        // Ensure the rootfs directory exists for runtimes that validate spec.root.path.
        if let Err(e) = std::fs::create_dir_all(&rootfs_path) {
            return Err(anyhow!("Failed to prepare rootfs directory {:?}: {}", rootfs_path, e));
        }

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
        // Check FORCE_DOCKER_CLI_RUNTIME env var for running in Docker container
        let force_docker_cli = std::env::var("FORCE_DOCKER_CLI_RUNTIME").is_ok();
        info!("[DEBUG] Selecting runtime: native={:?}, oci_image={:?}, force_docker_cli={}", manifest.compute.native, oci_image, force_docker_cli);
        let runtime: Arc<dyn Runtime> = if manifest.compute.native.is_some() {
            self.native_runtime.clone()
        } else if !oci_image.is_empty() {
            // Unit tests use RuntimeKind::Mock and should never shell out to Docker.
            if self.container_runtime.config().kind == crate::runtime::RuntimeKind::Mock {
                info!("[DEBUG] Using ContainerRuntime (Mock) for tests");
                self.container_runtime.clone()
            } else
            // On macOS or when FORCE_DOCKER_CLI_RUNTIME is set, use DockerCliRuntime
            // This is needed when Engine runs in a Docker container (Linux) but needs to
            // spawn containers via Docker socket
            if cfg!(target_os = "macos") || force_docker_cli {
                info!("[DEBUG] Using DockerCliRuntime for OCI image: {} (force={})", oci_image, force_docker_cli);
                self.docker_cli_runtime.clone()
            } else {
                info!("[DEBUG] Using ContainerRuntime for OCI image");
                self.container_runtime.clone()
            }
        } else {
            info!("[DEBUG] No native or OCI config, falling back to DevRuntime");
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

        // Start usage tracking
        self.usage_tracker.start_tracking(capsule_id.clone());

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
                user_id: None,
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
        entry.user_id = manifest.metadata.get("user_id").cloned();

        info!(
            capsule_id = capsule_id,
            pid = runtime.pid,
            bundle = %runtime.bundle_path.display(),
            "Recorded capsule runtime launch"
        );

        // Audit Log: CAPSULE_DEPLOY (async, fire-and-forget)
        let logger = self.audit_logger.clone();
        tokio::spawn(async move {
            logger.log_event(AuditOperation::DeployCapsule, AuditStatus::Success).await;
        });

        // Audit Log: CAPSULE_START (async, fire-and-forget)
        let logger = self.audit_logger.clone();
        tokio::spawn(async move {
            logger.log_event(AuditOperation::StartCapsule, AuditStatus::Success).await;
        });
        
        // Log details separately
        info!(
            "Started workload {} (pid={})",
            capsule_id, runtime.pid
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
                started_at: None,
                remote_url: None,
                user_id: None,
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

    /// Stop and remove a capsule. Returns whether VRAM scrubbing was performed successfully.
    pub async fn stop_capsule(&self, capsule_id: &str) -> Result<bool> {
        info!("Stopping capsule {}", capsule_id);

        // Remove any per-capsule egress policy.
        crate::security::EgressPolicyRegistry::global().unregister(capsule_id);

        let adep_json = {
            let capsules = self.capsules.read().map_err(|e| anyhow!("Lock error: {}", e))?;
            capsules.get(capsule_id).map(|c| c.adep_json.clone())
        };

        let manifest: Option<AdepManifest> = adep_json
            .as_ref()
            .and_then(|json| serde_json::from_slice(json).ok());

        if let Some(ref manifest) = manifest {
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

        // Log audit event (async, fire-and-forget)
        let logger = self.audit_logger.clone();
        tokio::spawn(async move {
            logger.log_event(AuditOperation::StopCapsule, AuditStatus::Success).await;
        });
        info!("Stopped capsule {}", capsule_id);

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

        // Stop usage tracking and report
        if let Some(start_time) = self.usage_tracker.stop_tracking(capsule_id) {
            let user_id = {
                let capsules = self.capsules.read().map_err(|e| anyhow!("Lock error: {}", e))?;
                capsules.get(capsule_id).and_then(|c| c.user_id.clone())
            };

            if let Some(uid) = user_id {
                if let Some(reporter) = &self.usage_reporter {
                    let end_time = std::time::SystemTime::now();
                    let reporter = reporter.clone();
                    let cid = capsule_id.to_string();
                    tokio::spawn(async move {
                        reporter.report(cid, uid, start_time, end_time).await;
                    });
                }
            }
        }

        // Cleanup storage if StorageManager is configured
        if let Some(storage_manager) = &self.storage_manager {
            info!("Cleaning up storage for capsule {}", capsule_id);
            match storage_manager.cleanup_capsule_storage(capsule_id) {
                Ok(_) => info!("Storage cleaned up for capsule {}", capsule_id),
                Err(e) => warn!("Failed to cleanup storage for capsule {}: {}", capsule_id, e),
            }
        }

        // Scrub VRAM if the capsule used a GPU
        let mut scrubbed = false;
        if let Some(manifest) = &manifest {
            if manifest.requires_gpu() {
                let gpu_indices = match self.gpu_detector.detect_gpus() {
                    Ok(report) => report.gpus.iter().map(|g| g.index as usize).collect::<Vec<_>>(),
                    Err(e) => {
                        warn!("Failed to detect GPUs for scrubbing: {}", e);
                        Vec::new()
                    }
                };

                if !gpu_indices.is_empty() {
                    let scrub_task = tokio::task::spawn_blocking(move || {
                        crate::security::vram_scrubber::scrub_gpu_indices(&gpu_indices, VramScrubber::new)
                    });

                    match scrub_task.await {
                        Ok(results) => {
                            for stats in results.iter() {
                                if let Some(msg) = &stats.message {
                                    warn!("Partial VRAM scrub on GPU {}: {}", stats.gpu_index, msg);
                                } else {
                                    info!("Scrubbed GPU {} bytes={} chunks={}", stats.gpu_index, stats.bytes_scrubbed, stats.chunks);
                                }
                            }
                            scrubbed = results.iter().any(|s| s.message.is_none() && s.bytes_scrubbed > 0);
                        }
                        Err(e) => {
                            warn!("VRAM scrubbing task failed: {}", e);
                        }
                    }
                }
            }
        }

        Ok(scrubbed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> CapsuleManager {
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
            Some(runtime_config),
            None, // usage_reporter
            None, // storage_config
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
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_capsule_not_found() {
        let manager = create_test_manager();
        let result = manager.stop_capsule("non-existent").await;
        assert!(result.is_err());
    }
}
