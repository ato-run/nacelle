//! Tests for HCL v3.0 manifest parsing
//!
//! Run with: `cargo test --features hcl-support`

#[cfg(feature = "hcl-support")]
mod tests {
    use libadep_core::capsule_manifest::CapsuleManifest;

    #[test]
    fn test_parse_minimal_hcl_manifest() {
        let hcl_content = r#"
capsule {
  name = "test-capsule"
  version = "1.0.0"
}
"#;
        let manifest: CapsuleManifest = hcl::from_str(hcl_content).unwrap();
        assert_eq!(manifest.capsule.name, "test-capsule");
        assert_eq!(manifest.capsule.version, "1.0.0");
    }

    #[test]
    fn test_parse_mlx_native_block() {
        let hcl_content = r#"
capsule {
  name = "mlx-inference"
  version = "1.0.0"
  description = "MLX inference capsule"
}

native {
  runtime = "mlx"
  model = "mlx-community/Qwen2.5-0.5B-Instruct-4bit"
  context_size = 4096
  port = 8081
}

resources {
  memory = "8GB"
  gpu_memory_min = "4GB"
}
"#;
        let manifest: CapsuleManifest = hcl::from_str(hcl_content).unwrap();

        assert_eq!(manifest.capsule.name, "mlx-inference");
        assert!(manifest.native.is_some());

        let native = manifest.native.as_ref().unwrap();
        assert_eq!(native.runtime, "mlx");
        assert_eq!(
            native.model,
            Some("mlx-community/Qwen2.5-0.5B-Instruct-4bit".to_string())
        );
        assert_eq!(native.context_size, Some(4096));
        assert_eq!(native.port, Some(8081));

        assert!(manifest.requires_mlx());
        assert!(!manifest.requires_llama());
        assert!(!manifest.requires_vllm());
    }

    #[test]
    fn test_parse_llama_native_block() {
        let hcl_content = r#"
capsule {
  name = "llama-server"
  version = "0.1.0"
}

native {
  runtime = "llama"
  model = "${GUMBALL_MODELS_DIR}/model.gguf"
  args = ["--ctx-size", "4096", "--n-gpu-layers", "99"]
}
"#;
        let manifest: CapsuleManifest = hcl::from_str(hcl_content).unwrap();

        assert!(manifest.native.is_some());
        let native = manifest.native.as_ref().unwrap();
        assert_eq!(native.runtime, "llama");

        assert!(manifest.requires_llama());
        assert!(!manifest.requires_mlx());
    }

    #[test]
    fn test_parse_vllm_native_block() {
        let hcl_content = r#"
capsule {
  name = "vllm-server"
  version = "1.0.0"
}

native {
  runtime = "vllm"
  model = "meta-llama/Llama-3.1-8B-Instruct"
  quantization = "awq"
}

resources {
  gpu_memory_min = "24GB"
}
"#;
        let manifest: CapsuleManifest = hcl::from_str(hcl_content).unwrap();

        assert!(manifest.native.is_some());
        let native = manifest.native.as_ref().unwrap();
        assert_eq!(native.runtime, "vllm");
        assert_eq!(native.quantization, Some("awq".to_string()));

        assert!(manifest.requires_vllm());
    }

    #[test]
    fn test_parse_with_permissions() {
        let hcl_content = r#"
capsule {
  name = "secure-agent"
  version = "1.0.0"
}

permissions {
  network_allow = ["https://api.openai.com", "https://api.anthropic.com"]
  files_allow = ["/data/*"]
  mcp_servers = ["filesystem", "memory"]
}
"#;
        let manifest: CapsuleManifest = hcl::from_str(hcl_content).unwrap();

        assert!(manifest.permissions.network_allow.is_some());
        let network = manifest.permissions.network_allow.as_ref().unwrap();
        assert_eq!(network.len(), 2);
        assert!(network.contains(&"https://api.openai.com".to_string()));

        let mcp = manifest.permissions.mcp_servers.as_ref().unwrap();
        assert!(mcp.contains(&"filesystem".to_string()));
    }

    #[test]
    fn test_parse_with_env_vars() {
        let hcl_content = r#"
capsule {
  name = "env-test"
  version = "1.0.0"
}

native {
  runtime = "mlx"
  env = {
    MLX_METAL = "1"
    TOKENIZERS_PARALLELISM = "false"
  }
}
"#;
        let manifest: CapsuleManifest = hcl::from_str(hcl_content).unwrap();

        let native = manifest.native.as_ref().unwrap();
        let env = native.env.as_ref().unwrap();
        assert_eq!(env.get("MLX_METAL"), Some(&"1".to_string()));
    }

    #[test]
    fn test_effective_runtime_prefers_native() {
        let hcl_content = r#"
capsule {
  name = "dual-config"
  version = "1.0.0"
}

native {
  runtime = "mlx"
}

runtime {
  type = "native"
  executable = "llama-server"
}
"#;
        let manifest: CapsuleManifest = hcl::from_str(hcl_content).unwrap();

        // Native block should take precedence
        assert_eq!(manifest.effective_runtime(), Some("mlx"));
    }

    #[test]
    fn test_full_gumball_manifest() {
        let hcl_content = r#"
# Gumball MLX Inference Capsule
# HCL v3.0 format

capsule {
  name        = "gumball-mlx"
  version     = "0.1.0"
  description = "Proactive AI Agent with MLX inference"
}

native {
  runtime      = "mlx"
  model        = "mlx-community/Qwen3-Next-80B-A3B-4bit"
  context_size = 32768
  port         = 8081
}

ai {
  base_model     = "mlx-community/Qwen3-Next-80B-A3B-4bit"
  fallback_device = "cpu"
}

resources {
  memory         = "16GB"
  gpu_memory_min = "8GB"
  cpu_cores      = 4
}

permissions {
  network_allow = ["*"]
  files_allow   = ["${HOME}/.gumball/*"]
  mcp_servers   = ["filesystem", "memory", "fetch"]
}

routing {
  internal_port = 8081
}
"#;
        let manifest: CapsuleManifest = hcl::from_str(hcl_content).unwrap();

        assert_eq!(manifest.capsule.name, "gumball-mlx");
        assert!(manifest.requires_mlx());

        let native = manifest.native.as_ref().unwrap();
        assert_eq!(native.context_size, Some(32768));

        assert_eq!(manifest.resources.cpu_cores, Some(4));
        assert_eq!(manifest.routing.internal_port, Some(8081));
    }
}
