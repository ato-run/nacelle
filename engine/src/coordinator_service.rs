use std::path::PathBuf;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

use crate::adep::{
    AdepManifest, AdepVolume, ComputeConfig as AdepComputeConfig,
    GpuConstraints as AdepGpuConstraints, SchedulingConfig as AdepSchedulingConfig,
};
use crate::capsule_manager::CapsuleManager;
use crate::oci::spec_builder::build_oci_spec;
use crate::proto::onescluster::coordinator::v1::{
    agent_service_server::AgentService as AgentServiceTrait, DeployWorkloadRequest, DeployWorkloadResponse,
    FetchModelRequest, FetchModelResponse, StopWorkloadRequest, StopWorkloadResponse,
};
use crate::runtime::{ContainerRuntime, LaunchRequest, RuntimeError};
use crate::runtime::traits::Runtime;
use crate::workload::manifest_loader::{load_manifest_str, ResourceRequirements};

/// AgentService implements the Agent gRPC service
///
/// This service handles requests from the Coordinator to the Agent:
/// 1. DeployWorkload - Coordinator instructs Agent to deploy a workload
/// 2. StopWorkload - Coordinator instructs Agent to stop a workload
pub struct AgentService {
    runtime: Arc<ContainerRuntime>,
    capsule_manager: Arc<CapsuleManager>,
    allowed_host_paths: Vec<String>,
}

impl AgentService {
    pub fn new(
        capsule_manager: Arc<CapsuleManager>,
        runtime: Arc<ContainerRuntime>,
        allowed_host_paths: Vec<String>,
    ) -> Self {
        Self {
            runtime,
            capsule_manager,
            allowed_host_paths,
        }
    }
}

// Helper functions removed (no longer using Proto manifest types)

#[tonic::async_trait]
impl AgentServiceTrait for AgentService {
    /// Coordinator instructs Agent to deploy a workload
    ///
    /// Flow:
    /// 1. Receive adep_manifest_json from Coordinator
    /// 2. Parse JSON to AdepManifest
    /// 3. Call OCI spec builder (Week 3) to generate config.json
    /// 4. In production: Write config.json to disk and call runc/crun
    /// 5. For simulation: Just validate that OCI spec can be generated
    /// 6. Return success/failure
    async fn deploy_workload(
        &self,
        request: Request<DeployWorkloadRequest>,
    ) -> Result<Response<DeployWorkloadResponse>, Status> {
        let req = request.into_inner();
        if req.workload_id.trim().is_empty() {
            return Err(Status::invalid_argument("workload_id is required"));
        }
        info!(
            "🚀 Received DeployWorkload request from Coordinator for workload {}",
            req.workload_id
        );

        if req.adep_json.is_empty() {
             return Err(Status::invalid_argument(
                 "DeployWorkloadRequest.adep_json must be provided",
             ));
        }

        let manifest_str = String::from_utf8(req.adep_json.clone())
            .map_err(|e| Status::invalid_argument(format!("Invalid UTF-8 in adep_json: {}", e)))?;

        let (manifest, resource_hints): (AdepManifest, Option<ResourceRequirements>) =
            match load_manifest_str(None, &manifest_str) {
                Ok((m, r)) => (m, r),
                Err(e) => {
                    return Err(Status::invalid_argument(format!(
                        "Invalid adep manifest: {}",
                        e
                    )))
                }
            };

        info!("  Workload: {}", manifest.name);
        info!("  Image: {}", manifest.compute.image);
        info!("  Requires GPU: {}", manifest.requires_gpu());

        let requires_gpu = manifest.requires_gpu();
        let rootfs_path = resolve_rootfs_path(&manifest)?;

        debug!(
            workload_id = req.workload_id,
            rootfs = %rootfs_path.display(),
            "Resolved rootfs for workload"
        );

        // TODO: Handle resource_assignment from request if available, currently missing in proto
        let resource_assignment = vec![]; 

        let oci_spec = build_oci_spec(
            &rootfs_path,
            &manifest.compute,
            &manifest.volumes,
            if requires_gpu {
                Some(&resource_assignment)
            } else {
                None
            },
            &self.allowed_host_paths,
            resource_hints.as_ref(),
        )
        .map_err(|e| Status::internal(format!("Failed to build OCI spec: {}", e)))?;

        let manifest_json_owned = String::from_utf8(req.adep_json).unwrap_or_default();

        let launch_result = match self
            .runtime
            .launch(LaunchRequest {
                workload_id: &req.workload_id,
                spec: &oci_spec,
                manifest_json: Some(&manifest_json_owned),
            })
            .await
        {
            Ok(result) => result,
            Err(err) => {
                if let Err(record_err) = self
                    .capsule_manager
                    .record_runtime_failure(&req.workload_id, &err)
                {
                    warn!(
                        workload_id = req.workload_id,
                        error = %record_err,
                        "Failed to record runtime failure"
                    );
                }
                return Err(runtime_error_to_status(&req.workload_id, err));
            }
        };

        let reserved_vram_bytes = manifest.required_vram_bytes();
        if let Err(err) = self.capsule_manager.record_runtime_launch(
            &req.workload_id,
            &manifest,
            &manifest_json_owned,
            &launch_result,
            reserved_vram_bytes,
        ) {
            error!(
                workload_id = req.workload_id,
                error = %err,
                "Failed to persist capsule metadata"
            );
            return Err(Status::internal(format!(
                "Workload '{}' launched but metadata persistence failed: {}",
                manifest.name, err
            )));
        }

        info!(
            workload_id = req.workload_id,
            pid = launch_result.pid,
            bundle = %launch_result.bundle_path.display(),
            "✅ Deployment successful"
        );

        // Construct WorkloadStatus
        let status = crate::proto::onescluster::coordinator::v1::WorkloadStatus {
            workload_id: req.workload_id,
            name: manifest.name,
            reserved_vram_bytes,
            phase: crate::proto::onescluster::coordinator::v1::WorkloadPhase::Running as i32,
            pid: launch_result.pid as u64,
            observed_vram_bytes: 0, // TODO
        };

        Ok(Response::new(DeployWorkloadResponse {
            workload_id: status.workload_id.clone(),
            status: Some(status),
        }))
    }

    async fn stop_workload(
        &self,
        request: Request<StopWorkloadRequest>,
    ) -> Result<Response<StopWorkloadResponse>, Status> {
        let req = request.into_inner();
        info!("StopWorkload request: workload_id={}", req.workload_id);

        match self.capsule_manager.stop_capsule(&req.workload_id).await {
            Ok(_) => {
                info!("Workload {} stopped successfully", req.workload_id);
                Ok(Response::new(StopWorkloadResponse {
                    workload_id: req.workload_id,
                    success: true,
                }))
            }
            Err(e) => {
                error!("Failed to stop workload {}: {}", req.workload_id, e);
                Ok(Response::new(StopWorkloadResponse {
                    workload_id: req.workload_id,
                    success: false,
                }))
            }
        }
    }

    /// Coordinator instructs Agent to fetch a model file
    async fn fetch_model(
        &self,
        request: Request<FetchModelRequest>,
    ) -> Result<Response<FetchModelResponse>, Status> {
        let req = request.into_inner();
        info!(
            "FetchModel request: url={}, dest={}",
            req.url, req.destination
        );

        // Validate URL (basic check)
        if req.url.trim().is_empty() {
            return Err(Status::invalid_argument("URL is required"));
        }

        // Validate destination path (security check is done inside downloader, but we can do it early too)
        // We'll let the downloader handle the strict validation and file operations.

        // Spawn the download task asynchronously so we don't block the gRPC thread
        // In a real system, we might want to track this task or return a job ID.
        // For MVP, we'll wait for it (since the prompt says "Simple RPC for MVP").
        // "非同期タスクとして実行し...今回はMVPとして、完了まで待つSimple RPCで構いません"
        // So we will await it here.

        match crate::downloader::download_file(&req.url, &req.destination, &self.allowed_host_paths)
            .await
        {
            Ok(bytes_downloaded) => {
                info!("Model fetched successfully: {}", req.destination);
                Ok(Response::new(FetchModelResponse {
                    success: true,
                    message: "Download completed successfully".to_string(),
                    bytes_downloaded,
                }))
            }
            Err(e) => {
                error!("Failed to fetch model: {}", e);
                Ok(Response::new(FetchModelResponse {
                    success: false,
                    message: format!("Download failed: {}", e),
                    bytes_downloaded: 0,
                }))
            }
        }
    }
}

fn resolve_rootfs_path(manifest: &AdepManifest) -> Result<PathBuf, Status> {
    if let Some(path) = manifest.metadata.get("rootfs_path") {
        let pb = PathBuf::from(path);
        if pb.exists() {
            return Ok(pb);
        }
        return Err(Status::failed_precondition(format!(
            "manifest.metadata.rootfs_path does not exist: {}",
            pb.display()
        )));
    }

    if let Ok(path) = std::env::var("CAPSULED_DEFAULT_ROOTFS") {
        let pb = PathBuf::from(path);
        if pb.exists() {
            return Ok(pb);
        }
        return Err(Status::failed_precondition(format!(
            "CAPSULED_DEFAULT_ROOTFS path does not exist: {}",
            pb.display()
        )));
    }

    Err(Status::failed_precondition(
        "rootfs_path metadata not provided and CAPSULED_DEFAULT_ROOTFS not set",
    ))
}

fn runtime_error_to_status(workload_id: &str, err: RuntimeError) -> Status {
    match err {
        RuntimeError::CommandFailure {
            operation,
            exit_code,
            stderr,
        } => {
            let message = format!(
                "Runtime '{}' failed for workload {} (exit={:?}): {}",
                operation,
                workload_id,
                exit_code,
                stderr.trim()
            );
            Status::internal(message)
        }
        RuntimeError::InvalidConfig(msg) => Status::failed_precondition(format!(
            "Runtime config error for {}: {}",
            workload_id, msg
        )),
        other => Status::internal(format!("Runtime error for {}: {}", workload_id, other)),
    }
}
