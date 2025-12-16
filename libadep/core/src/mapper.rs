use crate::capsule_v1::{CapsuleManifestV1, ValidationError};
use crate::error::CapsuleError;
use crate::runplan::RunPlan;

fn validation_errors_to_capsule_error(errors: Vec<ValidationError>) -> CapsuleError {
    let msg = errors
        .into_iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("; ");
    CapsuleError::ValidationError(msg)
}

/// Parse `capsule_v1` from TOML and validate against the frozen v1.0 spec.
pub fn parse_validate_capsule_v1_toml(content: &str) -> Result<CapsuleManifestV1, CapsuleError> {
    let manifest = CapsuleManifestV1::from_toml(content)?;
    manifest
        .validate()
        .map_err(validation_errors_to_capsule_error)?;
    Ok(manifest)
}

/// Convert canonical v1 TOML directly into a normalized `RunPlan`.
pub fn capsule_v1_toml_to_run_plan(content: &str) -> Result<RunPlan, CapsuleError> {
    let manifest = parse_validate_capsule_v1_toml(content)?;
    manifest.to_run_plan()
}

#[cfg(feature = "capsuled-proto")]
pub fn capsule_v1_toml_to_proto_run_plan(
    content: &str,
) -> Result<onescluster_capsuled_proto::onescluster::common::v1::RunPlan, CapsuleError> {
    let plan = capsule_v1_toml_to_run_plan(content)?;
    Ok(run_plan_to_proto(&plan))
}

#[cfg(feature = "capsuled-proto")]
pub fn run_plan_to_proto(
    plan: &RunPlan,
) -> onescluster_capsuled_proto::onescluster::common::v1::RunPlan {
    use onescluster_capsuled_proto::onescluster::common::v1 as proto;

    let runtime = match &plan.runtime {
        crate::runplan::RunPlanRuntime::Docker(r) => {
            proto::run_plan::Runtime::Docker(proto::DockerRuntime {
                image: r.image.clone(),
                digest: r.digest.clone().unwrap_or_default(),
                command: r.command.clone(),
                env: r.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                working_dir: r.working_dir.clone().unwrap_or_default(),
                user: r.user.clone().unwrap_or_default(),
                ports: r
                    .ports
                    .iter()
                    .map(|p| proto::Port {
                        container_port: p.container_port,
                        host_port: p.host_port.unwrap_or_default(),
                        protocol: p.protocol.clone().unwrap_or_default(),
                    })
                    .collect(),
                mounts: r
                    .mounts
                    .iter()
                    .map(|m| proto::Mount {
                        source: m.source.clone(),
                        target: m.target.clone(),
                        readonly: m.readonly,
                    })
                    .collect(),
            })
        }
        crate::runplan::RunPlanRuntime::Native(r) => {
            proto::run_plan::Runtime::Native(proto::NativeRuntime {
                binary_path: r.binary_path.clone(),
                args: r.args.clone(),
                env: r.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                working_dir: r.working_dir.clone().unwrap_or_default(),
            })
        }
        crate::runplan::RunPlanRuntime::PythonUv(r) => {
            proto::run_plan::Runtime::PythonUv(proto::PythonUvRuntime {
                entrypoint: r.entrypoint.clone(),
                args: r.args.clone(),
                env: r.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                working_dir: r.working_dir.clone().unwrap_or_default(),
                ports: r
                    .ports
                    .iter()
                    .map(|p| proto::Port {
                        container_port: p.container_port,
                        host_port: p.host_port.unwrap_or_default(),
                        protocol: p.protocol.clone().unwrap_or_default(),
                    })
                    .collect(),
            })
        }
    };

    proto::RunPlan {
        capsule_id: plan.capsule_id.clone(),
        name: plan.name.clone(),
        version: plan.version.clone(),
        cpu_cores: plan.cpu_cores.unwrap_or_default(),
        memory_bytes: plan.memory_bytes.unwrap_or_default(),
        gpu_profile: plan.gpu_profile.clone().unwrap_or_default(),
        egress_allowlist: plan.egress_allowlist.clone(),
        runtime: Some(runtime),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_TOML: &str = r#"
schema_version = "1.0"
name = "hello-docker"
version = "0.1.0"
type = "app"

[execution]
runtime = "docker"
entrypoint = "ghcr.io/example/hello:latest"
port = 8080
"#;

    #[test]
    fn parses_validates_and_converts_to_run_plan() {
        let plan = capsule_v1_toml_to_run_plan(VALID_TOML).unwrap();
        assert_eq!(plan.capsule_id, "hello-docker");
    }

    #[test]
    fn rejects_invalid_name() {
        let toml = VALID_TOML.replace("hello-docker", "Invalid");
        let err = capsule_v1_toml_to_run_plan(&toml).unwrap_err();
        match err {
            CapsuleError::ValidationError(msg) => assert!(msg.contains("Invalid name")),
            other => panic!("expected validation error, got: {other}"),
        }
    }

    #[cfg(feature = "capsuled-proto")]
    #[test]
    fn converts_to_proto_run_plan() {
        let plan = capsule_v1_toml_to_proto_run_plan(VALID_TOML).unwrap();
        assert_eq!(plan.name, "hello-docker");
        assert!(plan.runtime.is_some());
    }
}
