use anyhow::{anyhow, Result};
use std::path::Path;

use crate::adep::{AdepManifest, ComputeConfig, GpuConstraints, SchedulingConfig};

/// Resource requirements extracted from capsule or adep manifest
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct ResourceRequirements {
    pub cpu_cores: Option<u32>,
    pub memory_bytes: Option<u64>,
    pub gpu_memory_bytes: Option<u64>,
}


/// Parse a manifest that may be either JSON (adep) or TOML (capsule.toml).
/// Returns `AdepManifest` convertible to the engine's internal format and an
/// optional `ResourceRequirements` with hints extracted from TOML or metadata.
pub fn load_manifest_str(
    path: Option<&Path>,
    text: &str,
) -> Result<(AdepManifest, Option<ResourceRequirements>)> {
    // If extension suggests TOML, parse TOML first
    if let Some(p) = path {
        if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            if ext.eq_ignore_ascii_case("toml") {
                return load_toml_manifest(text);
            }
        }
    }

    // Try JSON first
    match serde_json::from_str::<AdepManifest>(text) {
        Ok(manifest) => Ok((manifest, None)),
        Err(_) => {
            // Try TOML (feature-gated by libadep-core toml-support dependency)
            load_toml_manifest(text)
        }
    }
}

fn load_toml_manifest(text: &str) -> Result<(AdepManifest, Option<ResourceRequirements>)> {
    #[cfg(feature = "toml-support")]
    {
        use libadep_core::capsule_manifest::CapsuleManifest;
        use libadep_core::utils::parse_memory_string;

        let capsule: CapsuleManifest = CapsuleManifest::from_toml_str(text)
            .map_err(|e| anyhow!("failed to parse TOML: {}", e))?;

        // Convert to AdepManifest and resource hints
        let manifest: AdepManifest = capsule.clone().into();

        // Build resource requirements
        let mut req = ResourceRequirements::default();
        req.cpu_cores = capsule.resources.cpu_cores;
        if let Some(mem) = &capsule.resources.memory {
            if let Ok(bytes) = parse_memory_string(mem) {
                req.memory_bytes = Some(bytes);
            }
        }
        if let Some(vram) = &capsule.resources.gpu_memory_min {
            if let Ok(bytes) = parse_memory_string(vram) {
                req.gpu_memory_bytes = Some(bytes);
            }
        }

        Ok((manifest, Some(req)))
    }

    #[cfg(not(feature = "toml-support"))]
    {
        Err(anyhow!("TOML support is not enabled in this build"))
    }
}

impl From<libadep_core::capsule_manifest::CapsuleManifest> for AdepManifest {
    fn from(c: libadep_core::capsule_manifest::CapsuleManifest) -> Self {
        // Mapping policy: map capsule.name -> adep.name, and create a minimal compute
        // entry so the Agent can produce an OCI spec. Engines should provide a
        // default image for capsule-based workloads (placeholder).
        let image = if let Some(_base) = c.ai.base_model.clone() {
            // If base_model is present, prefer an AI runner image
            "onescluster/ai-runner:latest".to_string()
        } else {
            // Default placeholder image for capsules without explicit compute
            "onescluster/local-capsule:latest".to_string()
        };

        // Build scheduling with GPU hints (we set vram_min_gb if provided)
        let gpu_constraints = c.resources.gpu_memory_min.as_ref().and_then(|s| {
            match libadep_core::utils::parse_memory_string(s) {
                Ok(bytes) => Some(GpuConstraints {
                    vram_min_gb: bytes / (1024 * 1024 * 1024),
                    cuda_version_min: None,
                }),
                Err(_) => None,
            }
        });

        let cloud_constraints = if c.resources.cloud_accelerators.is_some()
            || c.resources.cloud_region.is_some()
            || c.resources.allowed_clouds.is_some()
        {
            Some(crate::adep::CloudConstraints {
                accelerators: c.resources.cloud_accelerators,
                region: c.resources.cloud_region,
                allowed_clouds: c.resources.allowed_clouds,
            })
        } else {
            None
        };

        let scheduling = SchedulingConfig {
            gpu: gpu_constraints,
            strategy: None,
            cloud: cloud_constraints,
        };

        AdepManifest {
            name: c.capsule.name,
            scheduling,
            compute: ComputeConfig {
                image,
                args: vec![],
                env: vec![],
            },
            volumes: vec![],
            metadata: std::collections::HashMap::new(),
        }
    }
}
