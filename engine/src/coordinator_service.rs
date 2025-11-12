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
    coordinator_server::Coordinator, AdePManifest as ProtoAdePManifest, DeployWorkloadRequest,
    DeployWorkloadResponse, SchedulingConfig as ProtoSchedulingConfig, StatusReportRequest,
    StatusReportResponse,
};
use crate::runtime::{ContainerRuntime, LaunchRequest, RuntimeError};

/// CoordinatorService implements the Coordinator gRPC service for Agent
///
/// This service handles requests from the Coordinator to the Agent:
/// 1. DeployWorkload - Coordinator instructs Agent to deploy a workload (Week 4)
pub struct CoordinatorService {
    runtime: Arc<ContainerRuntime>,
    capsule_manager: Arc<CapsuleManager>,
}

impl CoordinatorService {
    pub fn new(capsule_manager: Arc<CapsuleManager>, runtime: Arc<ContainerRuntime>) -> Self {
        Self {
            runtime,
            capsule_manager,
        }
    }
}

fn convert_manifest(proto: &ProtoAdePManifest) -> Result<AdepManifest, Status> {
    if proto.name.is_empty() {
        return Err(Status::invalid_argument("manifest.name is required"));
    }

    let scheduling = convert_scheduling(proto.scheduling.as_ref());

    let compute_proto = proto
        .compute
        .clone()
        .ok_or_else(|| Status::invalid_argument("manifest.compute is required"))?;

    if compute_proto.image.is_empty() {
        return Err(Status::invalid_argument(
            "manifest.compute.image is required",
        ));
    }

    let env_pairs: Vec<String> = if compute_proto.env.is_empty() {
        Vec::new()
    } else {
        let mut entries: Vec<_> = compute_proto.env.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries
            .into_iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect()
    };

    let volumes = proto
        .volumes
        .iter()
        .map(|volume| AdepVolume {
            r#type: volume.r#type.clone(),
            source: volume.source.clone(),
            destination: volume.destination.clone(),
            readonly: volume.readonly,
        })
        .collect();

    Ok(AdepManifest {
        name: proto.name.clone(),
        scheduling,
        compute: AdepComputeConfig {
            image: compute_proto.image,
            args: compute_proto.args,
            env: env_pairs,
        },
        volumes,
        metadata: proto.metadata.clone(),
    })
}

fn convert_scheduling(proto: Option<&ProtoSchedulingConfig>) -> AdepSchedulingConfig {
    match proto {
        Some(config) => {
            let gpu = config.gpu.as_ref().map(|gpu| AdepGpuConstraints {
                vram_min_gb: gpu.vram_min_gb,
                cuda_version_min: if gpu.cuda_version_min.is_empty() {
                    None
                } else {
                    Some(gpu.cuda_version_min.clone())
                },
            });

            let strategy = if config.strategy.trim().is_empty() {
                None
            } else {
                Some(config.strategy.clone())
            };

            AdepSchedulingConfig { gpu, strategy }
        }
        None => AdepSchedulingConfig {
            gpu: None,
            strategy: None,
        },
    }
}

#[tonic::async_trait]
impl Coordinator for CoordinatorService {
    /// Coordinator currently does not accept status reports from the Coordinator->Agent stream.
    ///
    /// Agents originate ReportStatus calls toward the Coordinator (Go service),
    /// so the Agent-side gRPC server returns Unimplemented to callers.
    async fn report_status(
        &self,
        _request: Request<StatusReportRequest>,
    ) -> Result<Response<StatusReportResponse>, Status> {
        Err(Status::unimplemented(
            "ReportStatus should be invoked against the Coordinator service",
        ))
    }

    /// Coordinator instructs Agent to deploy a workload (Week 4)
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

        let manifest = match req.manifest.as_ref() {
            Some(proto_manifest) => convert_manifest(proto_manifest).or_else(|err| {
                if req.manifest_json.is_empty() {
                    Err(err)
                } else {
                    serde_json::from_str::<AdepManifest>(&req.manifest_json).map_err(|json_err| {
                        Status::invalid_argument(format!(
                            "Invalid adep manifest (typed error: {err}; json error: {json_err})"
                        ))
                    })
                }
            })?,
            None => {
                if req.manifest_json.is_empty() {
                    return Err(Status::invalid_argument(
                        "DeployWorkloadRequest.manifest or manifest_json must be provided",
                    ));
                }
                serde_json::from_str::<AdepManifest>(&req.manifest_json)
                    .map_err(|err| Status::invalid_argument(format!("Invalid adep.json: {err}")))?
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

        let oci_spec = build_oci_spec(
            &rootfs_path,
            &manifest.compute,
            &manifest.volumes,
            requires_gpu,
        )
        .map_err(|e| Status::internal(format!("Failed to build OCI spec: {}", e)))?;

        let manifest_json_owned = if req.manifest_json.is_empty() {
            serde_json::to_string(&manifest)
                .map_err(|err| Status::internal(format!("Failed to serialize manifest: {err}")))?
        } else {
            req.manifest_json.clone()
        };

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

        Ok(Response::new(DeployWorkloadResponse {
            success: true,
            message: format!(
                "Workload '{}' deployed (pid={}, bundle={})",
                manifest.name,
                launch_result.pid,
                launch_result.bundle_path.display()
            ),
        }))
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
