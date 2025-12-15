use crate::adep::{AdepManifest, AdepVolume, ComputeConfig, GpuConstraints, NativeConfig, SchedulingConfig};
use crate::proto::onescluster::common::v1 as common;

/// Result of converting a RunPlan proto into the legacy Adep manifest that the engine can launch.
pub struct RunPlanConversion {
    pub adep: AdepManifest,
    pub oci_image: String,
    pub digest: String,
}

/// Convert coordinator RunPlan proto into an Adep manifest with best-effort field mapping.
pub fn from_coordinator(plan: &common::RunPlan) -> RunPlanConversion {
    let name = if !plan.name.is_empty() {
        plan.name.clone()
    } else if !plan.capsule_id.is_empty() {
        plan.capsule_id.clone()
    } else {
        "capsule".to_string()
    };

    let mut manifest = AdepManifest {
        name: name.clone(),
        scheduling: SchedulingConfig::default(),
        compute: ComputeConfig::default(),
        volumes: Vec::new(),
        metadata: Default::default(),
    };

    // Preserve gpu_profile as metadata hint; vram_min_gb is unknown format so we avoid guessing.
    if !plan.gpu_profile.is_empty() {
        manifest
            .metadata
            .insert("gpu_profile".to_string(), plan.gpu_profile.clone());
    }

    // Resource hints
    if let Some(cpu) = to_option_u32(plan.cpu_cores) {
        manifest
            .metadata
            .insert("cpu_cores".to_string(), cpu.to_string());
    }
    if let Some(mem) = to_option_u64(plan.memory_bytes) {
        manifest
            .metadata
            .insert("memory_bytes".to_string(), mem.to_string());
    }

    // Optional GPU constraint from profile if it looks like "<number>GB"
    if let Some(vram_gb) = parse_vram_gb_hint(&plan.gpu_profile) {
        manifest.scheduling.gpu = Some(GpuConstraints {
            vram_min_gb: vram_gb,
            cuda_version_min: None,
        });
    }

    // Common helpers
    let mut env = Vec::new();
    if let Some(runtime_env) = runtime_env(plan) {
        env.extend(runtime_env);
    }

    let mut oci_image = String::new();
    let mut digest = String::new();

    match &plan.runtime {
        Some(common::run_plan::Runtime::Docker(docker)) => {
            oci_image = docker.image.clone();
            digest = docker.digest.clone();

            manifest.compute.image = docker.image.clone();
            manifest.compute.args = docker.command.clone();
            manifest.compute.env = env.clone();

            // First port becomes PORT env for compatibility
            if let Some(port_env) = first_port_env(&docker.ports) {
                manifest.compute.env.push(port_env.clone());
                ensure_env(&mut env, port_env);
            }

            // Map mounts into legacy volumes
            manifest.volumes = docker
                .mounts
                .iter()
                .map(|m| AdepVolume {
                    r#type: "bind".to_string(),
                    source: m.source.clone(),
                    destination: m.target.clone(),
                    readonly: m.readonly,
                })
                .collect();
        }
        Some(common::run_plan::Runtime::PythonUv(py)) => {
            // Represent python-uv as native runtime "uv" with entrypoint + args
            let mut args = Vec::new();
            args.push(py.entrypoint.clone());
            args.extend(py.args.clone());

            manifest.compute.native = Some(NativeConfig {
                runtime: "uv".to_string(),
                args,
            });
            manifest.compute.env = env.clone();

            if let Some(port_env) = first_port_env(&py.ports) {
                manifest.compute.env.push(port_env.clone());
                ensure_env(&mut env, port_env);
            }
        }
        Some(common::run_plan::Runtime::Native(native)) => {
            manifest.compute.native = Some(NativeConfig {
                runtime: native.binary_path.clone(),
                args: native.args.clone(),
            });
            manifest.compute.env = env.clone();
        }
        None => {
            // No runtime, leave defaults; caller will likely reject later.
            manifest.compute.env = env.clone();
        }
    }

    RunPlanConversion {
        adep: manifest,
        oci_image,
        digest,
    }
}

/// Convert engine RunPlan proto into an Adep manifest (engine uses the shared common RunPlan).
pub fn from_engine(plan: &common::RunPlan) -> RunPlanConversion {
    from_coordinator(plan)
}

fn runtime_env(plan: &common::RunPlan) -> Option<Vec<String>> {
    match &plan.runtime {
        Some(common::run_plan::Runtime::Docker(docker)) => {
            Some(env_map_to_vec(&docker.env))
        }
        Some(common::run_plan::Runtime::Native(native)) => {
            Some(env_map_to_vec(&native.env))
        }
        Some(common::run_plan::Runtime::PythonUv(py)) => {
            Some(env_map_to_vec(&py.env))
        }
        None => None,
    }
}

fn env_map_to_vec(map: &std::collections::HashMap<String, String>) -> Vec<String> {
    map.iter()
        .filter_map(|(k, v)| {
            if k.is_empty() {
                None
            } else {
                Some(format!("{}={}", k, v))
            }
        })
        .collect()
}

fn ensure_env(env: &mut Vec<String>, value: String) {
    if !env.iter().any(|existing| existing == &value) {
        env.push(value);
    }
}

fn first_port_env(ports: &[common::Port]) -> Option<String> {
    ports.first().map(|p| {
        let port_value = if p.host_port != 0 { p.host_port } else { p.container_port };
        format!("PORT={}", port_value)
    })
}

fn parse_vram_gb_hint(profile: &str) -> Option<u64> {
    // Accept simple suffix like "8GB" -> 8
    if let Some(stripped) = profile.strip_suffix("GB") {
        if let Ok(num) = stripped.trim().parse::<u64>() {
            return Some(num);
        }
    }
    None
}

fn to_option_u32(value: u32) -> Option<u32> {
    if value == 0 { None } else { Some(value) }
}

fn to_option_u64(value: u64) -> Option<u64> {
    if value == 0 { None } else { Some(value) }
}
