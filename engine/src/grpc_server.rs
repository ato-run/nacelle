use std::sync::Arc;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info, warn};

use crate::capsule_manager::CapsuleManager;
use crate::hardware::GpuDetector;
use crate::proto::onescluster::engine::v1::{
    deploy_request::Manifest as DeployManifest,
    engine_server::{Engine, EngineServer},
    CapsuleInfo, DeployRequest, DeployResponse, GetResourcesRequest, GetSystemStatusRequest,
    EngineLogEntry, ResourceInfo, ResourceUsage, StopRequest, StopResponse, SystemStatus,
    ValidateRequest, ValidationResult,
};
use crate::runtime::ContainerRuntime;
use crate::runplan;
use crate::wasm_host::AdepLogicHost;
use crate::workload;
use crate::artifact::ArtifactManager;
use crate::proto::onescluster::common::v1 as common;

use libadep_core::capsule_v1::CapsuleManifestV1;

/// EngineService implements the Engine gRPC service
use crate::network::service_registry::ServiceRegistry;
use crate::network::tailscale::TailscaleManager;

#[derive(Clone)]
pub struct EngineService {
    capsule_manager: Arc<CapsuleManager>,
    wasm_host: Arc<AdepLogicHost>,
    backend_mode: String,
    tailscale_manager: Arc<TailscaleManager>,
    service_registry: Arc<ServiceRegistry>,
    _runtime: Arc<ContainerRuntime>,
    _allowed_host_paths: Vec<String>,
    gpu_detector: Arc<dyn GpuDetector>,
}

impl EngineService {
    pub fn new(
        capsule_manager: Arc<CapsuleManager>,
        wasm_host: Arc<AdepLogicHost>,
        backend_mode: String,
        tailscale_manager: Arc<TailscaleManager>,
        service_registry: Arc<ServiceRegistry>,
        runtime: Arc<ContainerRuntime>,
        allowed_host_paths: Vec<String>,
        gpu_detector: Arc<dyn GpuDetector>,
        artifact_manager: Arc<ArtifactManager>,
    ) -> Self {
        Self {
            capsule_manager,
            wasm_host,
            backend_mode,
            tailscale_manager,
            service_registry,
            _runtime: runtime,
            _allowed_host_paths: allowed_host_paths,
            gpu_detector,
        }
    }
}

fn canonical_runplan_to_proto(plan: &libadep_core::runplan::RunPlan) -> common::RunPlan {
    use std::collections::HashMap;

    let runtime = match &plan.runtime {
        libadep_core::runplan::RunPlanRuntime::Docker(docker) => {
            let env: HashMap<String, String> = docker
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            common::run_plan::Runtime::Docker(common::DockerRuntime {
                image: docker.image.clone(),
                digest: docker.digest.clone().unwrap_or_default(),
                command: docker.command.clone(),
                env,
                working_dir: docker.working_dir.clone().unwrap_or_default(),
                user: docker.user.clone().unwrap_or_default(),
                ports: docker
                    .ports
                    .iter()
                    .map(|p| common::Port {
                        container_port: p.container_port,
                        host_port: p.host_port.unwrap_or(0),
                        protocol: p.protocol.clone().unwrap_or_default(),
                    })
                    .collect(),
                mounts: docker
                    .mounts
                    .iter()
                    .map(|m| common::Mount {
                        source: m.source.clone(),
                        target: m.target.clone(),
                        readonly: m.readonly,
                    })
                    .collect(),
            })
        }
        libadep_core::runplan::RunPlanRuntime::Native(native) => {
            let env: HashMap<String, String> = native
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            common::run_plan::Runtime::Native(common::NativeRuntime {
                binary_path: native.binary_path.clone(),
                args: native.args.clone(),
                env,
                working_dir: native.working_dir.clone().unwrap_or_default(),
            })
        }
        libadep_core::runplan::RunPlanRuntime::PythonUv(py) => {
            let env: HashMap<String, String> = py
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            common::run_plan::Runtime::PythonUv(common::PythonUvRuntime {
                entrypoint: py.entrypoint.clone(),
                args: py.args.clone(),
                env,
                working_dir: py.working_dir.clone().unwrap_or_default(),
                ports: py
                    .ports
                    .iter()
                    .map(|p| common::Port {
                        container_port: p.container_port,
                        host_port: p.host_port.unwrap_or(0),
                        protocol: p.protocol.clone().unwrap_or_default(),
                    })
                    .collect(),
            })
        }
    };

    common::RunPlan {
        capsule_id: plan.capsule_id.clone(),
        name: plan.name.clone(),
        version: plan.version.clone(),
        runtime: Some(runtime),
        cpu_cores: plan.cpu_cores.unwrap_or(0),
        memory_bytes: plan.memory_bytes.unwrap_or(0),
        gpu_profile: plan.gpu_profile.clone().unwrap_or_default(),
        egress_allowlist: plan.egress_allowlist.clone(),
    }
}



fn try_canonical_toml_to_runplan_proto(toml_str: &str) -> Option<common::RunPlan> {
    let canonical = CapsuleManifestV1::from_toml(toml_str).ok()?;
    canonical.validate().ok()?;
    let canonical_plan = canonical.to_run_plan().ok()?;
    Some(canonical_runplan_to_proto(&canonical_plan))
}

#[tonic::async_trait]
impl Engine for EngineService {
    /// Deploy a new capsule
    async fn deploy_capsule(
        &self,
        request: Request<DeployRequest>,
    ) -> Result<Response<DeployResponse>, Status> {
        let req = request.into_inner();
        info!("DeployCapsule request: capsule_id={}", req.capsule_id);

        // Parse manifest (RunPlan preferred, fallback to TOML/JSON)
        let mut oci_image = req.oci_image.clone();
        let mut digest = req.digest.clone();
        let mut direct_command: Option<Vec<String>> = None;

        let _manifest_bytes = match req.manifest {
            Some(DeployManifest::RunPlan(plan)) => {
                info!("  Using RunPlan manifest");

                // If the caller provided an explicit command (argv) for Docker, pass it to DirectRuntime.
                if let Some(crate::proto::onescluster::common::v1::run_plan::Runtime::Docker(d)) =
                    plan.runtime.as_ref()
                {
                    if !d.command.is_empty() {
                        direct_command = Some(d.command.clone());
                    }
                }

                let converted = runplan::from_engine(&plan);
                if oci_image.is_empty() {
                    oci_image = converted.oci_image;
                }
                if digest.is_empty() {
                    digest = converted.digest;
                }
                serde_json::to_vec(&converted.adep)
                    .map_err(|e| Status::internal(format!("Failed to serialize RunPlan: {}", e)))?
            }
            Some(DeployManifest::TomlContent(toml_str)) => {
                info!("  Parsing TOML manifest");
                // Canonical-first: capsule_v1 -> validated -> normalized RunPlan v0 -> same path as DeployManifest::RunPlan
                if let Some(plan) = try_canonical_toml_to_runplan_proto(&toml_str) {
                    if let Some(crate::proto::onescluster::common::v1::run_plan::Runtime::Docker(d)) =
                        plan.runtime.as_ref()
                    {
                        if direct_command.is_none() && !d.command.is_empty() {
                            direct_command = Some(d.command.clone());
                        }
                    }

                    let converted = runplan::from_engine(&plan);
                    if oci_image.is_empty() {
                        oci_image = converted.oci_image;
                    }
                    if digest.is_empty() {
                        digest = converted.digest;
                    }

                    serde_json::to_vec(&converted.adep)
                        .map_err(|e| Status::internal(format!("Failed to serialize RunPlan: {}", e)))?
                } else {
                    // Legacy fallback: Convert TOML to JSON for CapsuleManager
                    let (manifest, _) = workload::manifest_loader::load_manifest_str(None, &toml_str)
                        .map_err(|e| Status::invalid_argument(format!("Failed to parse TOML: {}", e)))?;

                    serde_json::to_vec(&manifest)
                        .map_err(|e| Status::internal(format!("Failed to serialize manifest: {}", e)))?
                }
            }
            Some(DeployManifest::AdepJson(json_bytes)) => {
                info!("  Using JSON manifest");
                json_bytes
            }
            None => {
                return Err(Status::invalid_argument(
                    "manifest is required (run_plan, toml_content, or adep_json)",
                ));
            }
        };

        if !digest.is_empty() {
            info!("  Using digest hint: {}", digest);
        }

        info!("  Delegating to CapsuleManager (Cloud Bursting Logic)...");

        let capsule_id = req.capsule_id.clone();
        let capsule_manager = self.capsule_manager.clone();
        tokio::spawn(async move {
                // Note: manifest_signature field not in proto - signature verification is internal
                let signature: Option<Vec<u8>> = None;

                match capsule_manager
                .deploy_capsule(capsule_id.clone(), _manifest_bytes, oci_image, digest, direct_command, signature)
                .await
            {
                Ok(status) => info!("Capsule {} deploy completed: {}", capsule_id, status),
                Err(e) => error!("Capsule {} deploy failed: {}", capsule_id, e),
            }
        });

        Ok(Response::new(DeployResponse {
            capsule_id: req.capsule_id,
            status: "starting".to_string(),
            local_url: String::new(),
        }))

    }

    /// Stop a running capsule
    async fn stop_capsule(
        &self,
        request: Request<StopRequest>,
    ) -> Result<Response<StopResponse>, Status> {
        let req = request.into_inner();
        info!("StopCapsule request: capsule_id={}", req.capsule_id);

        match self.capsule_manager.stop_capsule(&req.capsule_id).await {
            Ok(_scrubbed) => {
                info!("Capsule {} stopped successfully", req.capsule_id);
                Ok(Response::new(StopResponse {
                    capsule_id: req.capsule_id,
                    status: "stopped".to_string(),
                }))
            }
            Err(e) => {
                error!("Failed to stop capsule {}: {}", req.capsule_id, e);
                Err(Status::internal(format!("Stop failed: {}", e)))
            }
        }
    }

    /// Get resource information about this node
    async fn get_resources(
        &self,
        _request: Request<GetResourcesRequest>,
    ) -> Result<Response<ResourceInfo>, Status> {
        info!("GetResources request");

        // TODO: Get actual system resources
        // For now, return mock data
        let mut resources = get_system_resources();
        resources.backend_mode = self.backend_mode.clone();

        // Expose registered local service ports (capsule_id -> host port).
        resources.local_services = self
            .service_registry
            .get_services()
            .into_iter()
            .map(|s| (s.name, s.port as u32))
            .collect();

        info!(
            "Resources: cpu_cores={}, memory_bytes={}, disk_bytes={}, backend_mode={}",
            resources.cpu_cores,
            resources.memory_bytes,
            resources.disk_bytes,
            resources.backend_mode
        );

        Ok(Response::new(resources))
    }

    /// Validate a manifest without deploying
    async fn validate_manifest(
        &self,
        request: Request<ValidateRequest>,
    ) -> Result<Response<ValidationResult>, Status> {
        let req = request.into_inner();
        info!("ValidateManifest request ({} bytes)", req.adep_json.len());

        let adep_json_str = String::from_utf8(req.adep_json)
            .map_err(|e| Status::invalid_argument(format!("Invalid UTF-8 in adep_json: {}", e)))?;

        match self.wasm_host.validate_manifest(&adep_json_str) {
            Ok(_) => {
                info!("Manifest validation succeeded");
                Ok(Response::new(ValidationResult {
                    valid: true,
                    error_message: String::new(),
                }))
            }
            Err(e) => {
                warn!("Manifest validation failed: {}", e);
                Ok(Response::new(ValidationResult {
                    valid: false,
                    error_message: e.to_string(),
                }))
            }
        }
    }

    /// Get system status including capsules and resources
    async fn get_system_status(
        &self,
        _request: Request<GetSystemStatusRequest>,
    ) -> Result<Response<SystemStatus>, Status> {
        info!("GetSystemStatus request");

        // Get VPN IP
        let vpn_ip = self.tailscale_manager.get_vpn_ip().unwrap_or_default();

        // Get capsule list
        let capsules = match self.capsule_manager.list_capsules() {
            Ok(caps) => caps
                .into_iter()
                .map(|c| {
                    let local_url = self
                        .service_registry
                        .get_services()
                        .iter()
                        .find(|s| s.name == c.id)
                        .map(|s| format!("http://127.0.0.1:{}", s.port))
                        .unwrap_or_default();

                    CapsuleInfo {
                        id: c.id.clone(),
                        name: c.id,
                        status: c.status.to_string(),
                        local_url,
                        reserved_vram_bytes: c.reserved_vram_bytes,
                    }
                })
                .collect(),
            Err(e) => {
                warn!("Failed to list capsules: {}", e);
                vec![]
            }
        };

        // Get resource usage
        let hardware_report = self.gpu_detector.detect_gpus().unwrap_or_else(|_| {
            crate::hardware::RigHardwareReport {
                rig_id: "unknown".to_string(),
                gpus: vec![],
                system_cuda_version: None,
                system_driver_version: None,
                is_mock: false,
            }
        });

        let cpu_cores_total = num_cpus::get() as u64;
        let system_resources = get_system_resources();
        let memory_bytes_total = system_resources.memory_bytes;
        let vram_bytes_total = (hardware_report.total_vram_gb() * 1024.0 * 1024.0 * 1024.0) as u64;

        // Calculate used resources (simplified - just sum of reserved)
        let vram_bytes_used: u64 = capsules.iter().map(|c| c.reserved_vram_bytes).sum();

        let resources = ResourceUsage {
            cpu_cores_total,
            cpu_cores_used: capsules.len() as u64, // Simplified
            memory_bytes_total,
            memory_bytes_used: 0, // TODO: Calculate actual usage
            vram_bytes_total,
            vram_bytes_used,
        };

        Ok(Response::new(SystemStatus {
            backend_mode: self.backend_mode.clone(),
            vpn_ip,
            capsules,
            resources: Some(resources),
        }))
    }

    type StreamLogsStream = std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<EngineLogEntry, Status>> + Send + Sync + 'static>>;

    async fn stream_logs(
        &self,
        request: Request<crate::proto::onescluster::engine::v1::LogRequest>,
    ) -> Result<Response<Self::StreamLogsStream>, Status> {
        let req = request.into_inner();
        let capsule_id = req.capsule_id;
        let follow = req.follow;

        info!("StreamLogs request for capsule_id={}", capsule_id);

        // Get log path
        let log_path = self.capsule_manager.get_capsule_log_path(&capsule_id)
            .ok_or_else(|| Status::not_found("Capsule not found or no log path"))?;

        // Create a channel for the stream
        let (tx, rx) = tokio::sync::mpsc::channel(128);

        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;

            // Open file
            let file = match tokio::fs::File::open(&log_path).await {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(Err(Status::internal(format!("Failed to open log file: {}", e)))).await;
                    return;
                }
            };

            let mut reader = tokio::io::BufReader::new(file);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        // EOF
                        if follow {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            continue;
                        } else {
                            break;
                        }
                    }
                    Ok(_) => {
                        let entry = EngineLogEntry {
                            line: line.clone(),
                            timestamp: "".to_string(), // TODO: Parse timestamp if possible
                            source: "stdout".to_string(), // NativeRuntime mixes stdout/stderr
                        };
                        if tx.send(Ok(entry)).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(Status::internal(format!("Error reading log: {}", e)))).await;
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))))
    }
}

/// Get system resource information
fn get_system_resources() -> ResourceInfo {
    // TODO: Implement actual system resource detection
    // This would use sysinfo or similar crates to get real values

    #[cfg(target_os = "linux")]
    {
        // On Linux, we could read /proc/cpuinfo, /proc/meminfo, etc.
        ResourceInfo {
            cpu_cores: num_cpus::get() as u64,
            memory_bytes: get_total_memory(),
            disk_bytes: get_total_disk_space(),
            backend_mode: "unknown".to_string(), // Will be overwritten
            vpn_ip: String::new(),
            local_services: std::collections::HashMap::new(),
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Mock data for non-Linux systems
        ResourceInfo {
            cpu_cores: 4,
            memory_bytes: 8 * 1024 * 1024 * 1024, // 8 GB
            disk_bytes: 100 * 1024 * 1024 * 1024, // 100 GB
            backend_mode: "unknown".to_string(),  // Will be overwritten
            vpn_ip: String::new(),
            local_services: std::collections::HashMap::new(),
        }
    }
}

#[cfg(target_os = "linux")]
fn get_total_memory() -> u64 {
    // Read from /proc/meminfo
    if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                if let Some(kb_str) = line.split_whitespace().nth(1) {
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        return kb * 1024; // Convert KB to bytes
                    }
                }
            }
        }
    }
    8 * 1024 * 1024 * 1024 // Default 8 GB
}

#[cfg(target_os = "linux")]
fn get_total_disk_space() -> u64 {
    // This is a simplified version - in production you'd check actual mount points
    use std::fs;
    if let Ok(metadata) = fs::metadata("/") {
        // This doesn't give us total disk space, just a placeholder
        return 100 * 1024 * 1024 * 1024; // Default 100 GB
    }
    100 * 1024 * 1024 * 1024
}

/// Start the gRPC server
pub async fn start_grpc_server(
    addr: &str,
    capsule_manager: Arc<CapsuleManager>,
    wasm_host: Arc<AdepLogicHost>,
    runtime: Arc<ContainerRuntime>,
    allowed_host_paths: Vec<String>,
    backend_mode: String,
    tailscale_manager: Arc<TailscaleManager>,
    service_registry: Arc<ServiceRegistry>,
    gpu_detector: Arc<dyn GpuDetector>,
    artifact_manager: Arc<ArtifactManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = addr.parse()?;
    let engine_service = EngineService::new(
        Arc::clone(&capsule_manager),
        Arc::clone(&wasm_host),
        backend_mode,
        Arc::clone(&tailscale_manager),
        Arc::clone(&service_registry),
        Arc::clone(&runtime),
        allowed_host_paths.clone(),
        gpu_detector,
        artifact_manager,
    );
    info!("Engine gRPC server listening on {}", addr);

    Server::builder()
        .add_service(EngineServer::new(engine_service.clone()))
        .serve(addr)
        .await?;

    Ok(())
}
