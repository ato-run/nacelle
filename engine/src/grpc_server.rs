use std::sync::Arc;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{error, info, warn};

use crate::capsule_manager::CapsuleManager;
use crate::coordinator_service::AgentService;
use crate::proto::onescluster::coordinator::v1::agent_service_server::AgentServiceServer;
use crate::proto::onescluster::engine::v1::{
    engine_server::{Engine, EngineServer},
    DeployRequest, DeployResponse, GetResourcesRequest, ResourceInfo, StopRequest, StopResponse,
    ValidateRequest, ValidationResult,
};
use crate::runtime::ContainerRuntime;
use crate::wasm_host::AdepLogicHost;

/// EngineService implements the Engine gRPC service
pub struct EngineService {
    capsule_manager: Arc<CapsuleManager>,
    wasm_host: Arc<AdepLogicHost>,
}

impl EngineService {
    pub fn new(capsule_manager: Arc<CapsuleManager>, wasm_host: Arc<AdepLogicHost>) -> Self {
        Self {
            capsule_manager,
            wasm_host,
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
        info!(
            "DeployCapsule request: capsule_id={}, oci_image={}",
            req.capsule_id, req.oci_image
        );

        // Validate manifest first
        let adep_json_str = String::from_utf8(req.adep_json.clone())
            .map_err(|e| Status::invalid_argument(format!("Invalid UTF-8 in adep_json: {}", e)))?;

        if let Err(e) = self.wasm_host.validate_manifest(&adep_json_str) {
            warn!("Manifest validation failed for {}: {}", req.capsule_id, e);
            return Err(Status::invalid_argument(format!(
                "Manifest validation failed: {}",
                e
            )));
        }

        // Deploy the capsule
        match self
            .capsule_manager
            .deploy_capsule(
                req.capsule_id.clone(),
                req.adep_json,
                req.oci_image,
                req.digest,
            )
            .await
        {
            Ok(status) => {
                info!("Capsule {} deployed successfully", req.capsule_id);
                Ok(Response::new(DeployResponse {
                    capsule_id: req.capsule_id,
                    status,
                }))
            }
            Err(e) => {
                error!("Failed to deploy capsule {}: {}", req.capsule_id, e);
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
        let resources = get_system_resources();

        info!(
            "Resources: cpu_cores={}, memory_bytes={}, disk_bytes={}",
            resources.cpu_cores, resources.memory_bytes, resources.disk_bytes
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
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Mock data for non-Linux systems
        ResourceInfo {
            cpu_cores: 4,
            memory_bytes: 8 * 1024 * 1024 * 1024, // 8 GB
            disk_bytes: 100 * 1024 * 1024 * 1024, // 100 GB
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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = addr.parse()?;
    let engine_service = EngineService::new(Arc::clone(&capsule_manager), Arc::clone(&wasm_host));
    let agent_service = AgentService::new(capsule_manager, runtime, allowed_host_paths);

    info!("Engine gRPC server listening on {}", addr);

    Server::builder()
        .add_service(EngineServer::new(engine_service))
        .add_service(AgentServiceServer::new(agent_service))
        .serve(addr)
        .await?;

    Ok(())
}
