/// Adep (Application Definition and Execution Policy) module
///
/// This module defines the structure of adep.json manifests that describe
/// how capsules should be scheduled and executed in the One'sCluster environment.
///
/// The manifest is divided into sections:
/// - `scheduling`: Requirements for Coordinator's GPU-aware scheduler (Week 2)
/// - `compute`: Container image and execution parameters for Agent's OCI spec generation (Week 3)
/// - `volumes`: Model file mounting (e.g., GGUF files for LLM inference)
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete adep.json manifest structure
///
/// Example adep.json:
/// ```json
/// {
///   "name": "llama-inference",
///   "scheduling": {
///     "strategy": "best_fit",
///     "gpu": {
///       "vram_min_gb": 16,
///       "cuda_version_min": "12.0"
///     }
///   },
///   "compute": {
///     "image": "vllm/vllm-openai:latest",
///     "args": ["--model", "/models/llama-3-8b.gguf"],
///     "env": ["VLLM_API_KEY=secret"]
///   },
///   "volumes": [
///     {
///       "type": "bind",
///       "source": "/mnt/models/llama-3-8b.gguf",
///       "destination": "/models/llama-3-8b.gguf",
///       "readonly": true
///     }
///   ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdepManifest {
    /// Capsule name (unique identifier)
    pub name: String,

    /// Scheduling requirements (used by Coordinator)
    pub scheduling: SchedulingConfig,

    /// Container execution configuration (used by Agent)
    pub compute: ComputeConfig,

    /// Volume mounts (optional, for model files)
    #[serde(default)]
    pub volumes: Vec<AdepVolume>,

    /// Arbitrary metadata (optional)
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl AdepManifest {
    /// Check if this workload requires GPU resources
    pub fn requires_gpu(&self) -> bool {
        self.scheduling
            .gpu
            .as_ref()
            .map(|gpu| gpu.vram_min_gb > 0 || gpu.cuda_version_min.is_some())
            .unwrap_or(false)
    }

    /// Get the required VRAM in bytes
    pub fn required_vram_bytes(&self) -> u64 {
        self.scheduling
            .gpu
            .as_ref()
            .map(|gpu| gpu.vram_min_gb * 1024 * 1024 * 1024)
            .unwrap_or(0)
    }
}

/// Scheduling configuration block
///
/// This section defines constraints that the Coordinator's GPU-aware scheduler
/// uses to select an appropriate Rig (Agent node) for placement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchedulingConfig {
    #[serde(default)]
    pub gpu: Option<GpuConstraints>,

    #[serde(default)]
    pub strategy: Option<String>,

    #[serde(default)]
    pub cloud: Option<CloudConstraints>,
}

/// Cloud bursting constraints
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub struct CloudConstraints {
    /// Cloud accelerator type (e.g., "L4:1")
    pub accelerators: Option<String>,
    /// Preferred cloud region (e.g., "us-east")
    pub region: Option<String>,
    /// List of allowed cloud providers (e.g., ["runpod", "aws"])
    pub allowed_clouds: Option<Vec<String>>,
}

/// GPU resource constraints for scheduling
///
/// These constraints map directly to the Coordinator's `GpuConstraints` type (Week 2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub struct GpuConstraints {
    /// Minimum required VRAM in gigabytes (0 = CPU-only workload)
    #[serde(default)]
    pub vram_min_gb: u64,

    /// Minimum required CUDA version (e.g., "12.0")
    /// None = no CUDA requirement
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cuda_version_min: Option<String>,
}


/// Container compute configuration
///
/// This section defines how the capsule should be executed as an OCI container.
/// The Agent uses this information to generate the OCI config.json.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComputeConfig {
    /// OCI container image (e.g., "docker.io/vllm/vllm-openai:latest")
    pub image: String,

    /// Command-line arguments passed to the container entrypoint
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables in "KEY=VALUE" format
    #[serde(default)]
    pub env: Vec<String>,
}

/// Volume mount configuration
///
/// Defines how host files (e.g., GGUF model files) are mounted into the container.
/// This is essential for LLM inference where model files are stored on the host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdepVolume {
    /// Mount type (currently only "bind" is supported)
    #[serde(rename = "type")]
    pub r#type: String,

    /// Host path (source)
    pub source: String,

    /// Container path (destination)
    pub destination: String,

    /// Read-only flag (default: true for model files)
    #[serde(default = "default_readonly")]
    pub readonly: bool,
}

fn default_readonly() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adep_manifest_parse() {
        let json = r#"{
            "name": "test-capsule",
            "scheduling": {
                "strategy": "best_fit",
                "gpu": {
                    "vram_min_gb": 16,
                    "cuda_version_min": "12.0"
                }
            },
            "compute": {
                "image": "test/image:latest",
                "args": ["--arg1", "value1"],
                "env": ["KEY=VALUE"]
            },
            "volumes": [
                {
                    "type": "bind",
                    "source": "/host/path",
                    "destination": "/container/path",
                    "readonly": true
                }
            ]
        }"#;

        let manifest: AdepManifest = serde_json::from_str(json).unwrap();

        assert_eq!(manifest.name, "test-capsule");
        assert_eq!(manifest.scheduling.strategy.as_deref(), Some("best_fit"));
        assert_eq!(manifest.scheduling.gpu.as_ref().unwrap().vram_min_gb, 16);
        assert_eq!(
            manifest.scheduling.gpu.as_ref().unwrap().cuda_version_min,
            Some("12.0".to_string())
        );
        assert!(manifest.requires_gpu());
        assert_eq!(manifest.required_vram_bytes(), 16 * 1024 * 1024 * 1024);
        assert_eq!(manifest.compute.image, "test/image:latest");
        assert_eq!(manifest.compute.args.len(), 2);
        assert_eq!(manifest.volumes.len(), 1);
        assert!(manifest.volumes[0].readonly);
        assert!(manifest.metadata.is_empty());
    }

    #[test]
    fn test_adep_manifest_cpu_only() {
        let json = r#"{
            "name": "cpu-capsule",
            "scheduling": {},
            "compute": {
                "image": "hello-world:latest",
                "args": []
            }
        }"#;

        let manifest: AdepManifest = serde_json::from_str(json).unwrap();

        assert!(!manifest.requires_gpu());
        assert_eq!(manifest.required_vram_bytes(), 0);
        assert_eq!(manifest.volumes.len(), 0);
        assert!(manifest.scheduling.gpu.is_none());
    }

    #[test]
    fn test_gpu_constraints_default() {
        let constraints = GpuConstraints::default();
        assert_eq!(constraints.vram_min_gb, 0);
        assert_eq!(constraints.cuda_version_min, None);
    }

    #[test]
    fn test_volume_readonly_default() {
        let json = r#"{
            "type": "bind",
            "source": "/host",
            "destination": "/container"
        }"#;

        let volume: AdepVolume = serde_json::from_str(json).unwrap();
        assert!(volume.readonly); // Should default to true
    }
}
