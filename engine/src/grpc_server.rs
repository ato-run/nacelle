use std::sync::Arc;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info, warn};

use crate::capsule_manager::CapsuleManager;
use crate::coordinator_service::AgentService;
use crate::hardware::GpuDetector;
use crate::oci;
use crate::proto::onescluster::coordinator::v1::agent_service_server::AgentServiceServer;
use crate::proto::onescluster::engine::v1::{
    deploy_request::Manifest as DeployManifest,
    engine_server::{Engine, EngineServer},
    CapsuleInfo, DeployRequest, DeployResponse, GetResourcesRequest, GetSystemStatusRequest,
    ResourceInfo, ResourceUsage, StopRequest, StopResponse, SystemStatus, ValidateRequest,
    ValidationResult,
};
use crate::runtime::ContainerRuntime;
use crate::wasm_host::AdepLogicHost;
use crate::workload;

/// EngineService implements the Engine gRPC service
use crate::network::service_registry::ServiceRegistry;
use crate::network::tailscale::TailscaleManager;

pub struct EngineService {
    capsule_manager: Arc<CapsuleManager>,
    wasm_host: Arc<AdepLogicHost>,
    backend_mode: String,
    tailscale_manager: Arc<TailscaleManager>,
    service_registry: Arc<ServiceRegistry>,
    runtime: Arc<ContainerRuntime>,
    allowed_host_paths: Vec<String>,
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
        }
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
    );
    let agent_service = AgentService::new(capsule_manager, runtime, allowed_host_paths);

    info!("Engine gRPC server listening on {}", addr);

    Server::builder()
        .add_service(EngineServer::new(engine_service))
        .add_service(AgentServiceServer::new(agent_service))
        .serve(addr)
        .await?;

    Ok(())
}
