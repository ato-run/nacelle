use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::utils::parse_memory_string;

#[cfg(feature = "hcl-support")]
extern crate hcl;

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsuleManifest {
    pub capsule: CapsuleMetadata,

    #[serde(default)]
    pub resources: Resources,

    #[serde(default)]
    pub ai: AiConfig,

    #[serde(default)]
    pub permissions: Permissions,

    #[serde(default)]
    pub routing: Routing,

    #[serde(default)]
    pub ui: Option<UiConfig>,

    #[serde(default)]
    pub rag: Option<RagConfig>,

    #[serde(default)]
    pub runtime: Option<RuntimeConfig>,

    /// HCL v3.0: Native runtime configuration block
    /// Example: `native { runtime = "mlx" }`
    #[serde(default)]
    pub native: Option<NativeRuntimeConfig>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeConfig {
    #[serde(rename = "type")]
    pub runtime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// HCL v3.0: Native runtime configuration block
/// Supports: `native { runtime = "mlx" | "llama" | "vllm" }`
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NativeRuntimeConfig {
    /// Runtime type: "mlx", "llama", "vllm", or custom
    pub runtime: String,
    
    /// Optional model path or ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    
    /// Optional model quantization format (e.g., "4bit", "8bit")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    
    /// Optional context window size
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_size: Option<u32>,
    
    /// Optional port override
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    
    /// Additional arguments passed to the runtime
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    
    /// Environment variables for the runtime
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,

    /// Fallback configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FallbackConfig>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FallbackConfig {
    pub runtime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsuleMetadata {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Resources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_memory_min: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_accelerators: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_clouds: Option<Vec<String>>,
}

impl Resources {
    pub fn get_memory_bytes(&self) -> Result<u64, String> {
        match &self.memory {
            Some(s) => parse_memory_string(s).map_err(|e| e.to_string()),
            None => Ok(0),
        }
    }

    pub fn get_gpu_memory_bytes(&self) -> Result<u64, String> {
        match &self.gpu_memory_min {
            Some(s) => parse_memory_string(s).map_err(|e| e.to_string()),
            None => Ok(0),
        }
    }
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_device: Option<String>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Permissions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_allow: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_allow: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<String>>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Routing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_port: Option<u16>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub welcome_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_questions: Option<Vec<String>>,
}

#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RagConfig {
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default)]
    pub chunk_size: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
}

impl CapsuleManifest {
    #[cfg(feature = "toml-support")]
    pub fn from_toml_str(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    #[cfg(feature = "toml-support")]
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, anyhow::Error> {
        let content = fs::read_to_string(path)?;
        let manifest: Self = toml::from_str(&content)?;
        Ok(manifest)
    }

    /// Parse HCL v3.0 manifest format
    /// Supports syntax like:
    /// ```hcl
    /// capsule {
    ///   name = "my-capsule"
    ///   version = "1.0.0"
    /// }
    /// native {
    ///   runtime = "mlx"
    ///   model = "mlx-community/Qwen2.5-0.5B-Instruct-4bit"
    /// }
    /// ```
    #[cfg(feature = "hcl-support")]
    pub fn from_hcl_str(s: &str) -> Result<Self, hcl::Error> {
        hcl::from_str(s)
    }

    /// Load manifest from HCL file
    #[cfg(feature = "hcl-support")]
    pub fn load_from_hcl_file<P: AsRef<Path>>(path: P) -> Result<Self, anyhow::Error> {
        let content = fs::read_to_string(path)?;
        let manifest: Self = hcl::from_str(&content)?;
        Ok(manifest)
    }

    /// Get the effective runtime type (from native block or runtime config)
    pub fn effective_runtime(&self) -> Option<&str> {
        // Prefer native block (HCL v3.0 style)
        if let Some(native) = &self.native {
            return Some(&native.runtime);
        }
        // Fall back to runtime config (legacy TOML style)
        if let Some(runtime) = &self.runtime {
            if runtime.runtime_type == "native" {
                return runtime.executable.as_deref();
            }
        }
        None
    }

    /// Check if this manifest requires MLX runtime
    pub fn requires_mlx(&self) -> bool {
        matches!(self.effective_runtime(), Some("mlx"))
    }

    /// Check if this manifest requires llama.cpp runtime
    pub fn requires_llama(&self) -> bool {
        matches!(self.effective_runtime(), Some("llama" | "llama-server" | "llama.cpp"))
    }

    /// Check if this manifest requires vLLM runtime
    pub fn requires_vllm(&self) -> bool {
        matches!(self.effective_runtime(), Some("vllm"))
    }
}
