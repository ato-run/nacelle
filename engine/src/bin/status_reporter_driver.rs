use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use tokio::time::sleep;

use capsuled_engine::adep::{
    AdepManifest, ComputeConfig as AdepComputeConfig, GpuConstraints,
    SchedulingConfig as AdepSchedulingConfig,
};
use capsuled_engine::capsule_manager::CapsuleManager;
use capsuled_engine::hardware::gpu_process_monitor::GpuProcessMonitorError;
use capsuled_engine::hardware::{GpuDetector, GpuInfo, GpuProcessMonitor, RigHardwareReport};
use capsuled_engine::proto::onescluster::coordinator::v1::Taint;
use capsuled_engine::runtime::LaunchResult;
use capsuled_engine::status_reporter::StatusReporter;

#[derive(Parser, Debug)]
#[command(
    about = "Trigger a single StatusReporter heartbeat for end-to-end tests",
    rename_all = "kebab-case"
)]
struct Args {
    /// Coordinator gRPC endpoint (e.g. http://127.0.0.1:50052)
    #[arg(long)]
    coordinator_endpoint: String,

    /// Rig identifier to report in the hardware snapshot
    #[arg(long, default_value = "rig-e2e-test")]
    rig_id: String,

    /// Capsule/workload identifier to register with the capsule manager
    #[arg(long, default_value = "capsule-e2e-test")]
    workload_id: String,

    /// PID to associate with the capsule when reporting status
    #[arg(long)]
    pid: u32,

    /// Reserved VRAM in bytes that the capsule requested
    #[arg(long)]
    reserved_vram_bytes: u64,

    /// Observed VRAM in bytes that NVML reported for the capsule PID
    #[arg(long)]
    observed_vram_bytes: u64,

    /// Total VRAM available on the rig (bytes)
    #[arg(long, default_value_t = 64 * 1024 * 1024 * 1024)]
    total_vram_bytes: u64,

    /// CUDA driver version for metadata (optional)
    #[arg(long, default_value = "test-driver-1.0")]
    cuda_driver_version: String,

    /// Number of heartbeat reports to emit before exiting
    #[arg(long, default_value_t = 2)]
    send_count: u32,

    /// Delay between successive reports (milliseconds)
    #[arg(long, default_value_t = 250)]
    interval_ms: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let capsule_manager = Arc::new(CapsuleManager::new());

    seed_capsule(
        &capsule_manager,
        &args.workload_id,
        args.pid,
        args.reserved_vram_bytes,
    )?;

    let gpu_detector = Arc::new(FixedGpuDetector {
        rig_id: args.rig_id.clone(),
        total_vram_bytes: args.total_vram_bytes,
        observed_used_vram: args.observed_vram_bytes,
        cuda_driver_version: args.cuda_driver_version.clone(),
    });

    let monitor = Arc::new(FixedProcessMonitor {
        usages: HashMap::from([(args.pid, args.observed_vram_bytes)]),
    });

    let mut reporter = StatusReporter::new(
        args.coordinator_endpoint,
        Duration::from_secs(1),
        Arc::clone(&capsule_manager),
        gpu_detector,
        monitor,
        Vec::<Taint>::new(),
    );

    for _ in 0..args.send_count {
        reporter
            .send_report_once()
            .await
            .context("status report failed")?;
        sleep(Duration::from_millis(args.interval_ms)).await;
    }

    Ok(())
}

fn seed_capsule(
    manager: &Arc<CapsuleManager>,
    workload_id: &str,
    pid: u32,
    reserved_vram: u64,
) -> Result<()> {
    // Build a minimal manifest to satisfy CapsuleManager bookkeeping.
    let manifest = AdepManifest {
        name: workload_id.to_string(),
        scheduling: AdepSchedulingConfig {
            gpu: Some(GpuConstraints {
                vram_min_gb: (reserved_vram / 1_073_741_824).max(1),
                cuda_version_min: None,
            }),
            strategy: None,
        },
        compute: AdepComputeConfig {
            image: "test/image:latest".to_string(),
            args: vec!["--arg".into()],
            env: vec![],
        },
        volumes: vec![],
        metadata: HashMap::new(),
    };

    let manifest_json = serde_json::to_string(&manifest)?;

    let temp_dir = std::env::temp_dir().join(format!("capsuled-e2e-{}", workload_id));
    std::fs::create_dir_all(&temp_dir)?;
    let bundle_path = temp_dir.join("bundle");
    let log_path = temp_dir.join("logs");
    std::fs::create_dir_all(&bundle_path)?;
    std::fs::create_dir_all(&log_path)?;

    let launch_result = LaunchResult {
        pid,
        bundle_path,
        log_path: log_path.join("capsule.log"),
    };

    manager
        .record_runtime_launch(
            workload_id,
            &manifest,
            &manifest_json,
            &launch_result,
            reserved_vram,
        )
        .context("failed to seed capsule state")
}

struct FixedGpuDetector {
    rig_id: String,
    total_vram_bytes: u64,
    observed_used_vram: u64,
    cuda_driver_version: String,
}

impl GpuDetector for FixedGpuDetector {
    fn detect_gpus(
        &self,
    ) -> Result<RigHardwareReport, capsuled_engine::hardware::GpuDetectionError> {
        let mut report = RigHardwareReport::new(self.rig_id.clone());
        report.is_mock = true;
        report.system_driver_version = Some(self.cuda_driver_version.clone());
        report.gpus.push(GpuInfo {
            index: 0,
            device_name: "Test GPU".into(),
            vram_total_bytes: self.total_vram_bytes,
            cuda_compute_capability: Some("8.0".into()),
            uuid: "GPU-TEST-UUID".into(),            vram_used_bytes: Some(self.observed_used_vram),
        });
        Ok(report)
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "FixedGpuDetector"
    }
}

struct FixedProcessMonitor {
    usages: HashMap<u32, u64>,
}

impl GpuProcessMonitor for FixedProcessMonitor {
    fn collect_usage_bytes(&self) -> Result<HashMap<u32, u64>, GpuProcessMonitorError> {
        Ok(self.usages.clone())
    }
}
