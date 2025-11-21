#[cfg(feature = "real-gpu")]
use cudarc::driver::{CudaContext, DriverError};
#[cfg(feature = "real-gpu")]
use nvml_wrapper::Nvml;
#[cfg(feature = "real-gpu")]
use std::process::Command;
use tracing::info;
#[cfg(feature = "real-gpu")]
use tracing::warn;

use crate::security::audit::AuditLogger;
use std::sync::Arc;

pub struct GpuScrubber {
    audit_logger: Arc<AuditLogger>,
}

impl GpuScrubber {
    pub fn new(audit_logger: Arc<AuditLogger>) -> Self {
        Self { audit_logger }
    }

    #[cfg(feature = "real-gpu")]
    fn kill_residual_processes(&self, gpu_index: usize) -> anyhow::Result<()> {
        let nvml = Nvml::init().map_err(|e| anyhow::anyhow!("Failed to init NVML: {:?}", e))?;
        let device = nvml
            .device_by_index(gpu_index as u32)
            .map_err(|e| anyhow::anyhow!("Failed to get device: {:?}", e))?;

        let processes = device
            .running_compute_processes()
            .map_err(|e| anyhow::anyhow!("Failed to get processes: {:?}", e))?;

        if processes.is_empty() {
            return Ok(());
        }

        info!(
            "Found {} residual processes on GPU {}. Killing...",
            processes.len(),
            gpu_index
        );

        for proc in processes {
            let pid = proc.pid;
            info!("Killing residual process PID: {}", pid);
            // kill -9 <pid>
            let status = Command::new("kill").arg("-9").arg(pid.to_string()).status();

            match status {
                Ok(s) if s.success() => info!("Successfully killed PID {}", pid),
                Ok(s) => warn!("Failed to kill PID {}: exit code {:?}", pid, s.code()),
                Err(e) => warn!("Failed to execute kill command for PID {}: {:?}", pid, e),
            }
        }

        // Wait a bit for processes to release resources
        std::thread::sleep(std::time::Duration::from_millis(500));

        Ok(())
    }

    /// 指定されたGPUインデックスのVRAMを物理洗浄する
    ///
    /// # Arguments
    /// * `gpu_index` - CUDAデバイスインデックス (0, 1, ...)
    ///
    /// # Returns
    /// * `Result<(), anyhow::Error>` - 成功時はOk
    #[cfg(feature = "real-gpu")]
    pub fn scrub_device(&self, gpu_index: usize) -> anyhow::Result<()> {
        info!("Starting VRAM scrub for GPU: {}", gpu_index);

        // 1. CUDAコンテキストの初期化
        // CudaContext::new(i) は内部で cuInit() と cuDeviceGet() -> cuCtxCreate() を行う
        let ctx = CudaContext::new(gpu_index)
            .map_err(|e| anyhow::anyhow!("CUDA init failed: {:?}", e))?;
        let stream = ctx.default_stream();

        // 2. 空きメモリの総量を取得
        // 注意: 厳密には他プロセスがいないことを確認してから行う必要があるが、
        // ここでは「確保できるだけ確保してゼロ埋めする」戦略をとる。
        let (free, total) = ctx
            .mem_get_info()
            .map_err(|e| anyhow::anyhow!("Failed to get mem info: {:?}", e))?;
        info!("GPU {}: Free {} / Total {} bytes", gpu_index, free, total);

        // 残留プロセスチェック (簡易版)
        // 本来はNVMLでプロセスリストを取得してkillすべきだが、
        // ここではメモリ使用率から推測して警告を出す。
        if free < total * 95 / 100 {
            warn!(
                "GPU {} seems to have residual processes (Free < 95%). Attempting to kill...",
                gpu_index
            );
            if let Err(e) = self.kill_residual_processes(gpu_index) {
                warn!("Failed to kill residual processes: {:?}", e);
            }
        }

        // 3. 安全マージンを残してメモリを確保
        // 全量確保(free)しようとすると、ドライバのオーバーヘッド等で失敗することがあるため、
        // 少し余裕(10MB)を持たせる。
        let margin = 10 * 1024 * 1024; // 10MB
        let alloc_size = if free > margin { free - margin } else { free };

        if alloc_size == 0 {
            warn!("GPU {}: No memory to scrub.", gpu_index);
            self.audit_logger.log_event(
                "VRAM_SCRUB",
                Some(&format!("GPU-{}", gpu_index)),
                "SKIPPED",
                Some("No memory to scrub".to_string()),
            )?;
            return Ok(());
        }

        info!(
            "GPU {}: Allocating {} bytes for scrubbing...",
            gpu_index, alloc_size
        );

        unsafe {
            // 4. デバイスメモリの確保とゼロ埋め (alloc_zeros)
            // cudarc 0.17+ では alloc_zeros が利用可能で、内部で非同期に確保とmemsetを行う
            // 戻り値の CudaSlice がドロップされるまでメモリは確保される
            let _mem = stream
                .alloc_zeros::<u8>(alloc_size)
                .map_err(|e| anyhow::anyhow!("Failed to alloc/scrub: {:?}", e))?;

            // 5. 同期して完了を待つ
            stream
                .synchronize()
                .map_err(|e| anyhow::anyhow!("Failed to sync: {:?}", e))?;
        }

        info!("VRAM scrub completed successfully for GPU: {}", gpu_index);

        self.audit_logger.log_event(
            "VRAM_SCRUB",
            Some(&format!("GPU-{}", gpu_index)),
            "SUCCESS",
            Some(format!("Scrubbed {} bytes", alloc_size)),
        )?;

        Ok(())
    }

    #[cfg(not(feature = "real-gpu"))]
    pub fn scrub_device(&self, gpu_index: usize) -> anyhow::Result<()> {
        info!(
            "(MOCK) VRAM scrub skipped for GPU: {} (real-gpu feature disabled)",
            gpu_index
        );

        self.audit_logger.log_event(
            "VRAM_SCRUB",
            Some(&format!("GPU-{}", gpu_index)),
            "SUCCESS",
            Some("Mock scrub completed".to_string()),
        )?;

        Ok(())
    }
    pub async fn scrub_all_gpus(&self) -> anyhow::Result<()> {
        #[cfg(feature = "real-gpu")]
        {
            // Detect GPUs using NVML
            let nvml = Nvml::init().map_err(|e| anyhow::anyhow!("Failed to init NVML: {:?}", e))?;
            let count = nvml
                .device_count()
                .map_err(|e| anyhow::anyhow!("Failed to get device count: {:?}", e))?;

            info!("Scrubbing all {} detected GPUs...", count);

            for i in 0..count {
                // In a real implementation, we might want to do this in parallel
                // For now, sequential is safer
                if let Err(e) = self.scrub_device(i as usize) {
                    warn!("Failed to scrub GPU {}: {:?}", i, e);
                }
            }
        }

        #[cfg(not(feature = "real-gpu"))]
        {
            info!("(MOCK) Scrubbing all GPUs...");
            self.scrub_device(0)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 注意: このテストは実際にGPUがある環境でのみ動作する
    // CI環境などでGPUがない場合はスキップする必要がある
    #[test]
    #[ignore]
    fn test_scrub_device_0() {
        let temp_dir = tempfile::tempdir().unwrap();
        let log_path = temp_dir.path().join("audit.log");
        let key_path = temp_dir.path().join("node_key.pem");
        let logger =
            Arc::new(AuditLogger::new(log_path, key_path, "test-node".to_string()).unwrap());
        let scrubber = GpuScrubber::new(logger);

        // GPU 0 を洗浄するテスト
        let result = scrubber.scrub_device(0);
        match result {
            Ok(_) => println!("Scrubbing successful"),
            Err(e) => println!("Scrubbing failed (no GPU?): {:?}", e),
        }
    }
}
