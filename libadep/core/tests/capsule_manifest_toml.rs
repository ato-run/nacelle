#![cfg(feature = "toml-support")]

use libadep_core::capsule_manifest::CapsuleManifest;
use libadep_core::capsule_manifest::Resources;
use libadep_core::utils::parse_memory_string;

#[test]
fn test_parse_valid_manifest() {
    let toml_data = include_str!("../examples/capsule.toml");
    let manifest = CapsuleManifest::from_toml_str(toml_data).unwrap();

    assert_eq!(manifest.capsule.name, "meeting-summarizer");
    assert_eq!(manifest.resources.gpu_memory_min.as_deref(), Some("8GB"));

    let gpu_bytes = manifest.resources.get_gpu_memory_bytes().unwrap();
    assert_eq!(gpu_bytes, 8 * 1024 * 1024 * 1024);
}

#[test]
fn test_cpu_only_manifest() {
    let toml_data = r#"
        [capsule]
        name = "cpu-agent"
        version = "0.0.1"

        [resources]
        gpu_memory_min = "0GB"
    "#;
    let manifest = CapsuleManifest::from_toml_str(toml_data).unwrap();
    assert_eq!(manifest.resources.get_gpu_memory_bytes().unwrap(), 0);
}

#[test]
fn test_parse_memory_string_helper() {
    assert_eq!(parse_memory_string("4GB").unwrap(), 4 * 1024 * 1024 * 1024);
    assert_eq!(parse_memory_string("512MB").unwrap(), 512 * 1024 * 1024);
    assert!(parse_memory_string("garbage").is_err());
}

#[test]
fn test_native_runtime_config() {
    let toml_data = r#"
[capsule]
name = "mlx-server"
version = "1.0.0"
description = "MLX-optimized LLM inference"

[native]
runtime = "mlx"
model = "mlx-community/Qwen2.5-0.5B-Instruct-4bit"
context_size = 4096
port = 8081

[native.env]
MLX_METAL = "1"

[resources]
memory = "8GB"
gpu_memory_min = "4GB"

[ai]
base_model = "mlx-community/Qwen2.5-0.5B-Instruct-4bit"
    "#;
    let manifest = CapsuleManifest::from_toml_str(toml_data).unwrap();
    
    assert_eq!(manifest.capsule.name, "mlx-server");
    assert!(manifest.native.is_some());
    
    let native = manifest.native.unwrap();
    assert_eq!(native.runtime, "mlx");
    assert_eq!(native.model.as_deref(), Some("mlx-community/Qwen2.5-0.5B-Instruct-4bit"));
    assert_eq!(native.port, Some(8081));
    assert!(native.env.is_some());
}
