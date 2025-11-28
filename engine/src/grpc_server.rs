use std::sync::Arc;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info, warn};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::capsule_manager::CapsuleManager;
use crate::hardware::GpuDetector;
use crate::oci;
use crate::proto::onescluster::coordinator::v1::engine_service_server::{EngineService as EngineServiceTrait, EngineServiceServer};
use crate::proto::onescluster::coordinator::v1::{
    EnsureRuntimeRequest, EnsureRuntimeResponse, ExecuteCapsuleRequest, ExecuteCapsuleResponse,
    HardwareInfo, TerminateCapsuleRequest, TerminationResult,
};
use crate::proto::onescluster::engine::v1::{
    deploy_request::Manifest as DeployManifest,
    engine_server::{Engine, EngineServer},
    CapsuleInfo, DeployRequest, DeployResponse, GetResourcesRequest, GetSystemStatusRequest,
    EngineLogEntry, LogRequest, ResourceInfo, ResourceUsage, StopRequest, StopResponse, SystemStatus,
    ValidateRequest, ValidationResult,
};
use crate::runtime::ContainerRuntime;
use crate::wasm_host::AdepLogicHost;
use crate::workload;
use crate::artifact::ArtifactManager;

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
    runtime: Arc<ContainerRuntime>,
    allowed_host_paths: Vec<String>,
    gpu_detector: Arc<dyn GpuDetector>,
    artifact_manager: Arc<ArtifactManager>,
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
            runtime,
            allowed_host_paths,
            gpu_detector,
            artifact_manager,
        }
    }
}

#[tonic::async_trait]
impl EngineServiceTrait for EngineService {
    async fn execute_capsule(
        &self,
        request: Request<ExecuteCapsuleRequest>,
    ) -> Result<Response<ExecuteCapsuleResponse>, Status> {
        let req = request.into_inner();
        info!("ExecuteCapsule request: capsule_id={}", req.capsule_id);

        // Parse manifest (TOML or JSON)
        let manifest_bytes = match req.manifest {
            Some(crate::proto::onescluster::coordinator::v1::execute_capsule_request::Manifest::TomlContent(toml_str)) => {
                info!("  Parsing TOML manifest");
                let (manifest, _) = workload::manifest_loader::load_manifest_str(None, &toml_str)
                    .map_err(|e| Status::invalid_argument(format!("Failed to parse TOML: {}", e)))?;
                
                serde_json::to_vec(&manifest)
                    .map_err(|e| Status::internal(format!("Failed to serialize manifest: {}", e)))?
            }
            Some(crate::proto::onescluster::coordinator::v1::execute_capsule_request::Manifest::AdepJson(json_bytes)) => {
                info!("  Using JSON manifest");
                json_bytes
            }
            None => {
                return Err(Status::invalid_argument(
                    "manifest is required (either toml_content or adep_json)",
                ));
            }
        };

        // TODO: Handle runtime_name and runtime_version from request
        // For now, we assume the CapsuleManager handles runtime selection or it's embedded in the manifest/logic
        // But actually, CapsuleManager might need to know about the requested runtime.
        // The current CapsuleManager.deploy_capsule signature is:
        // pub async fn deploy_capsule(&self, capsule_id: String, manifest_bytes: Vec<u8>, oci_image: Option<String>, digest: Option<String>)
        
        // We don't have oci_image/digest in ExecuteCapsuleRequest directly, but they might be in the manifest.
        // Or we should extract them.
        
        match self.capsule_manager
            .deploy_capsule(
                req.capsule_id.clone(),
                manifest_bytes,
                "".to_string(), // oci_image - extracted from manifest if needed
                "".to_string(), // digest
            )
            .await
        {
            Ok(result) => {
                let local_url = self
                    .service_registry
                    .get_services()
                    .iter()
                    .find(|s| s.name == req.capsule_id)
                    .map(|s| format!("http://localhost:{}", s.port))
                    .unwrap_or_else(|| format!("http://localhost"));

                // Get actual PID from CapsuleManager
                let pid = self.capsule_manager.get_capsule_pid(&req.capsule_id).unwrap_or(0);

                Ok(Response::new(ExecuteCapsuleResponse {
                    pid: pid as i32,
                    actual_port: 0, // TODO: Get actual port from service registry
                }))
            }
            Err(e) => {
                error!("Execution failed: {}", e);
                Err(Status::internal(format!("Execution failed: {}", e)))
            }
        }
    }

    async fn terminate_capsule(
        &self,
        request: Request<TerminateCapsuleRequest>,
    ) -> Result<Response<TerminationResult>, Status> {
        let req = request.into_inner();
        info!("TerminateCapsule request: capsule_id={}", req.capsule_id);

        match self.capsule_manager.stop_capsule(&req.capsule_id).await {
            Ok(_) => {
                Ok(Response::new(TerminationResult {
                    success: true,
                    exit_code: 0,
                    vram_scrubbed: false,
                }))
            }
            Err(e) => {
                error!("Failed to stop capsule {}: {}", req.capsule_id, e);
                Ok(Response::new(TerminationResult {
                    success: false,
                    exit_code: -1,
                    vram_scrubbed: false,
                }))
            }
        }
    }

    async fn get_hardware_info(
        &self,
        _request: Request<()>,
    ) -> Result<Response<HardwareInfo>, Status> {
        // Use gpu_detector to get info
        let report = self.gpu_detector.detect_gpus().unwrap_or_else(|_| {
             crate::hardware::RigHardwareReport {
                rig_id: "unknown".to_string(),
                gpus: vec![],
                system_cuda_version: None,
                system_driver_version: None,
                is_mock: false,
            }
        });

        // Convert to proto GpuInfo
        let gpus = report.gpus.iter().map(|g| {
            crate::proto::onescluster::coordinator::v1::GpuInfo {
                index: g.index as i32,
                name: g.device_name.clone(),
                driver_version: "unknown".to_string(),
                vram_total_bytes: (g.vram_gb() * 1024.0 * 1024.0 * 1024.0) as u64,
                vram_free_bytes: 0, // TODO
                utilization_percent: 0.0,
                temperature_celsius: 0.0,
                supported_runtime: 0, // Unspecified
            }
        }).collect();

        Ok(Response::new(HardwareInfo {
            os: 0, // Unspecified
            os_version: "unknown".to_string(),
            hostname: "unknown".to_string(),
            gpus,
            total_ram_bytes: 0, // TODO
            cpu_cores: num_cpus::get() as i32,
            cpu_model: "unknown".to_string(),
        }))
    }

    async fn ensure_runtime(
        &self,
        request: Request<EnsureRuntimeRequest>,
    ) -> Result<Response<EnsureRuntimeResponse>, Status> {
        let req = request.into_inner();
        info!("EnsureRuntime request: name={}, version={}", req.runtime_name, req.version);

        match self.artifact_manager.ensure_runtime(&req.runtime_name, &req.version, None).await {
            Ok(path) => {
                Ok(Response::new(EnsureRuntimeResponse {
                    already_cached: true,
                    local_path: path.to_string_lossy().to_string(),
                    download_bytes: 0,
                }))
            }
            Err(e) => {
                error!("Failed to ensure runtime: {}", e);
                Ok(Response::new(EnsureRuntimeResponse {
                    already_cached: false,
                    local_path: "".to_string(),
                    download_bytes: 0,
                }))
            }
        }
    }


    async fn scrub_vram(
        &self,
        _request: Request<crate::proto::onescluster::coordinator::v1::ScrubVramRequest>,
    ) -> Result<Response<crate::proto::onescluster::coordinator::v1::ScrubVramResponse>, Status> {
        // TODO: Implement actual VRAM scrubbing
        Ok(Response::new(crate::proto::onescluster::coordinator::v1::ScrubVramResponse {
            results: vec![],
        }))
    }
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

        // Parse manifest (TOML or JSON)
        let manifest_bytes = match req.manifest {
            Some(DeployManifest::TomlContent(toml_str)) => {
                info!("  Parsing TOML manifest");
                // Convert TOML to JSON for CapsuleManager
                let (manifest, _) = workload::manifest_loader::load_manifest_str(None, &toml_str)
                    .map_err(|e| Status::invalid_argument(format!("Failed to parse TOML: {}", e)))?;
                
                serde_json::to_vec(&manifest)
                    .map_err(|e| Status::internal(format!("Failed to serialize manifest: {}", e)))?
            }
            Some(DeployManifest::AdepJson(json_bytes)) => {
                info!("  Using JSON manifest");
                json_bytes
            }
            None => {
                return Err(Status::invalid_argument(
                    "manifest is required (either toml_content or adep_json)",
                ));
            }
        };

        info!("  Delegating to CapsuleManager (Cloud Bursting Logic)...");

        // Delegate to CapsuleManager - this will handle:
        // 1. VRAM checking
        // 2. Local vs Cloud decision
        // 3. Cloud bursting if needed
        match self.capsule_manager
            .deploy_capsule(
                req.capsule_id.clone(),
                manifest_bytes,
                req.oci_image.clone(),
                req.digest.clone(),
            )
            .await
        {
            Ok(result) => {
                // Result can be:
                // - "running" (local deployment)
                // - "cloud-job-123" or similar (cloud deployment)
                
                if result.starts_with("job-") || result.starts_with("cloud") || result.starts_with("sky") {
                    // Cloud deployment
                    info!("☁️ Cloud deployment initiated: {}", result);
                    Ok(Response::new(DeployResponse {
                        capsule_id: req.capsule_id,
                        status: result.clone(),
                        local_url: format!("cloud:{}", result),
                    }))
                } else {
                    // Local deployment
                    info!("💻 Local deployment successful");
                    
                    // Get local URL from ServiceRegistry
                    let local_url = self
                        .service_registry
                        .get_services()
                        .iter()
                        .find(|s| s.name == req.capsule_id)
                        .map(|s| format!("http://localhost:{}", s.port))
                        .unwrap_or_else(|| format!("http://localhost", ));

                    Ok(Response::new(DeployResponse {
                        capsule_id: req.capsule_id,
                        status: result,
                        local_url,
                    }))
                }
            }
            Err(e) => {
                error!("Deployment failed: {}", e);
                Err(Status::internal(format!("Deployment failed: {}", e)))
            }
        }
    }

    /// Stop a running capsule
    async fn stop_capsule(
        &self,
        request: Request<StopRequest>,
    ) -> Result<Response<StopResponse>, Status> {
        let req = request.into_inner();
        info!("StopCapsule request: capsule_id={}", req.capsule_id);

        match self.capsule_manager.stop_capsule(&req.capsule_id).await {
            Ok(_) => {
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
                        .map(|s| format!("http://{}.local:{}", c.id, s.port))
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
    // AgentService removed

    info!("Engine gRPC server listening on {}", addr);

    Server::builder()
        .add_service(EngineServer::new(engine_service.clone())) // EngineService now implements both traits? No, I need to clone or split?
        // Wait, EngineService implements both Engine (old) and EngineServiceTrait (new).
        // I can register both services using the same struct if it implements both traits.
        // But I need to clone it if I want to pass it to both.
        // EngineService struct contains Arcs so it should be cheap to clone if I derive Clone.
        // AgentService removed (legacy/redundant)
        .add_service(EngineServiceServer::new(engine_service.clone()))
        .serve(addr)
        .await?;

    Ok(())
}

// I need to add #[derive(Clone)] to EngineService
