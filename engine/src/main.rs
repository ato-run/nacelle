use clap::Parser;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::prelude::*;

use capsuled_engine::capsule_manager::CapsuleManager;
use capsuled_engine::config::{self, StatusTaint};
use capsuled_engine::grpc_server;
use capsuled_engine::hardware;
use capsuled_engine::proto::onescluster::coordinator::v1::{Taint, TaintEffect};
use capsuled_engine::runtime::{ContainerRuntime, RuntimeConfig as EngineRuntimeConfig};
use capsuled_engine::status_reporter::StatusReporter;
use capsuled_engine::wasm_host::AdepLogicHost;

const DEFAULT_SERVER_ADDR: &str = "0.0.0.0:50051";
const DEFAULT_WASM_PATH: &str =
    "../adep-logic/target/wasm32-unknown-unknown/release/adep_logic.wasm";
const DEFAULT_COORDINATOR_ADDR: &str = "http://127.0.0.1:50052";
const DEFAULT_STATUS_INTERVAL_SECS: u64 = 30;
const DEFAULT_CONFIG_PATH: &str = "config.toml";

/// Capsuled Engine - Agent for running capsules
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// gRPC server listen address
    #[arg(short, long, default_value = DEFAULT_SERVER_ADDR)]
    addr: String,

    /// Path to adep-logic Wasm file
    #[arg(
        short,
        long,
        default_value = DEFAULT_WASM_PATH
    )]
    wasm_path: String,

    /// Coordinator gRPC endpoint (host:port or URL)
    #[arg(long, default_value = DEFAULT_COORDINATOR_ADDR)]
    coordinator_addr: String,

    /// Interval in seconds between status reports
    #[arg(long, default_value_t = DEFAULT_STATUS_INTERVAL_SECS)]
    status_interval_secs: u64,

    /// Path to configuration file
    #[arg(long, default_value = DEFAULT_CONFIG_PATH)]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "capsuled_engine=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Capsuled Engine starting...");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));

    let file_config = match config::load_config(&args.config) {
        Ok(Some(cfg)) => {
            info!("Loaded configuration from {}", args.config);
            Some(cfg)
        }
        Ok(None) => {
            info!(
                "No configuration file found at {}; relying on CLI/default values",
                args.config
            );
            None
        }
        Err(err) => {
            warn!("Failed to load configuration {}: {err}", args.config);
            None
        }
    };

    let listen_addr = if args.addr != DEFAULT_SERVER_ADDR {
        args.addr.clone()
    } else {
        file_config
            .as_ref()
            .and_then(|cfg| cfg.server_listen_addr())
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| DEFAULT_SERVER_ADDR.to_string())
    };

    let wasm_path = if args.wasm_path != DEFAULT_WASM_PATH {
        args.wasm_path.clone()
    } else {
        file_config
            .as_ref()
            .and_then(|cfg| cfg.wasm_module_path())
            .map(|path| path.to_string())
            .unwrap_or_else(|| DEFAULT_WASM_PATH.to_string())
    };

    let coordinator_addr = if args.coordinator_addr != DEFAULT_COORDINATOR_ADDR {
        args.coordinator_addr.clone()
    } else {
        file_config
            .as_ref()
            .and_then(|cfg| cfg.coordinator_endpoint())
            .map(|endpoint| endpoint.to_string())
            .unwrap_or_else(|| DEFAULT_COORDINATOR_ADDR.to_string())
    };

    let status_interval_secs_raw = if args.status_interval_secs != DEFAULT_STATUS_INTERVAL_SECS {
        args.status_interval_secs
    } else {
        file_config
            .as_ref()
            .and_then(|cfg| cfg.status_interval_secs())
            .unwrap_or(DEFAULT_STATUS_INTERVAL_SECS)
    };
    let status_interval_secs = status_interval_secs_raw.max(5);

    let static_taints = file_config
        .as_ref()
        .map(|cfg| build_taints(cfg.status_taints()))
        .unwrap_or_default();

    let runtime_config =
        EngineRuntimeConfig::from_section(file_config.as_ref().and_then(|cfg| cfg.runtime()))
            .map_err(|err| anyhow::anyhow!("runtime configuration error: {err}"))?;
    let container_runtime = Arc::new(ContainerRuntime::new(runtime_config));
    let runtime_cfg_ref = container_runtime.config();
    info!(
        kind = ?runtime_cfg_ref.kind,
        binary = %runtime_cfg_ref.binary_path.display(),
        bundle_root = %runtime_cfg_ref.bundle_root.display(),
        state_root = %runtime_cfg_ref.state_root.display(),
        "Initialized container runtime"
    );

    // Detect GPU hardware
    info!("Detecting GPU hardware...");
    let gpu_detector = hardware::create_gpu_detector();
    let gpu_process_monitor = hardware::create_gpu_process_monitor();
    match gpu_detector.detect_gpus() {
        Ok(report) => {
            info!("Hardware detection completed:");
            info!("  Rig ID: {}", report.rig_id);
            info!("  GPU Count: {}", report.gpu_count());
            info!("  Total VRAM: {:.2} GB", report.total_vram_gb());
            if report.is_mock {
                info!("  Mode: Mock (set MOCK_GPU_COUNT, MOCK_VRAM_GB to configure)");
            } else {
                info!("  Mode: Real (NVML)");
            }
            if let Some(cuda) = &report.system_cuda_version {
                info!("  CUDA Version: {}", cuda);
            }
            for gpu in &report.gpus {
                info!(
                    "    GPU {}: {} ({:.2} GB VRAM)",
                    gpu.index,
                    gpu.device_name,
                    gpu.vram_gb()
                );
            }
        }
        Err(e) => {
            error!("GPU detection failed: {}", e);
            error!("Continuing without GPU support");
        }
    }

    // Initialize Wasm host
    info!("Loading Wasm module from: {}", wasm_path);
    let wasm_host = match AdepLogicHost::from_file(&wasm_path) {
        Ok(host) => {
            info!("Wasm module loaded successfully");
            Arc::new(host)
        }
        Err(e) => {
            error!("Failed to load Wasm module: {}", e);
            error!(
                "Please ensure the adep-logic Wasm file exists at: {}",
                wasm_path
            );
            return Err(e);
        }
    };

    // Initialize capsule manager
    info!("Initializing capsule manager...");
    let capsule_manager = Arc::new(CapsuleManager::new());

    // Start status reporter to periodically send heartbeat reports to Coordinator
    let status_reporter = StatusReporter::new(
        coordinator_addr.clone(),
        Duration::from_secs(status_interval_secs),
        Arc::clone(&capsule_manager),
        Arc::clone(&gpu_detector),
        Arc::clone(&gpu_process_monitor),
        static_taints,
    );
    let status_task = status_reporter.start();

    // Start gRPC server
    info!("Starting gRPC server on {}", listen_addr);
    let server_result = grpc_server::start_grpc_server(
        &listen_addr,
        Arc::clone(&capsule_manager),
        Arc::clone(&wasm_host),
        container_runtime,
    )
    .await;
    status_task.abort();
    server_result.map_err(|e| anyhow::anyhow!("gRPC server error: {}", e))?;

    Ok(())
}

fn build_taints(entries: &[StatusTaint]) -> Vec<Taint> {
    entries
        .iter()
        .filter_map(|entry| match parse_effect(&entry.effect) {
            Some(effect) => Some(Taint {
                key: entry.key.clone(),
                value: entry.value.clone(),
                effect: effect as i32,
            }),
            None => {
                warn!(
                    "Ignoring invalid taint effect '{}' for key '{}'. Supported values: Unspecified, NoSchedule, PreferNoSchedule",
                    entry.effect,
                    entry.key
                );
                None
            }
        })
        .collect()
}

fn parse_effect(effect: &str) -> Option<TaintEffect> {
    let trimmed = effect.trim();
    if trimmed.is_empty() {
        return Some(TaintEffect::NoSchedule);
    }

    let normalized = trimmed
        .replace('-', "_")
        .replace(' ', "_")
        .to_ascii_uppercase();

    match normalized.as_str() {
        "UNSPECIFIED" | "TAINT_EFFECT_UNSPECIFIED" => Some(TaintEffect::Unspecified),
        "NO_SCHEDULE" | "NOSCHEDULE" | "TAINT_EFFECT_NO_SCHEDULE" => Some(TaintEffect::NoSchedule),
        "PREFER_NO_SCHEDULE" | "PREFERNOSCHEDULE" | "TAINT_EFFECT_PREFER_NO_SCHEDULE" => {
            Some(TaintEffect::PreferNoSchedule)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_effect_allows_various_inputs() {
        assert_eq!(parse_effect(""), Some(TaintEffect::NoSchedule));
        assert_eq!(parse_effect("NoSchedule"), Some(TaintEffect::NoSchedule));
        assert_eq!(
            parse_effect("prefer-no-schedule"),
            Some(TaintEffect::PreferNoSchedule)
        );
        assert_eq!(
            parse_effect("taint_effect_unspecified"),
            Some(TaintEffect::Unspecified)
        );
        assert!(parse_effect("invalid").is_none());
    }

    #[test]
    fn build_taints_skips_invalid_entries() {
        let entries = vec![
            StatusTaint {
                key: "gpu".into(),
                value: "absent".into(),
                effect: "NoSchedule".into(),
            },
            StatusTaint {
                key: "region".into(),
                value: "us-east".into(),
                effect: "invalid".into(),
            },
        ];

        let taints = build_taints(&entries);
        assert_eq!(taints.len(), 1);
        assert_eq!(taints[0].key, "gpu");
        assert_eq!(taints[0].value, "absent");
        assert_eq!(taints[0].effect, TaintEffect::NoSchedule as i32);
    }
}
