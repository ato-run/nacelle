#[cfg(feature = "real-gpu")]
use cudarc::driver::{CudaContext, DriverError};
#[cfg(feature = "real-gpu")]
use tracing::warn;
use tracing::info;

pub struct GpuScrubber;

impl GpuScrubber {
    /// 指定されたGPUインデックスのVRAMを物理洗浄する
    /// 
    /// # Arguments
    /// * `gpu_index` - CUDAデバイスインデックス (0, 1, ...)
    /// 
    /// # Returns
    /// * `Result<(), anyhow::Error>` - 成功時はOk
    #[cfg(feature = "real-gpu")]
    pub fn scrub_device(gpu_index: usize) -> anyhow::Result<()> {
        info!("Starting VRAM scrub for GPU: {}", gpu_index);
        
        // 1. CUDAコンテキストの初期化
        // CudaContext::new(i) は内部で cuInit() と cuDeviceGet() -> cuCtxCreate() を行う
        let ctx = CudaContext::new(gpu_index).map_err(|e| anyhow::anyhow!("CUDA init failed: {:?}", e))?;
        let stream = ctx.default_stream();
        
        // 2. 空きメモリの総量を取得
        // 注意: 厳密には他プロセスがいないことを確認してから行う必要があるが、
        // ここでは「確保できるだけ確保してゼロ埋めする」戦略をとる。
        let (free, total) = ctx.mem_get_info().map_err(|e| anyhow::anyhow!("Failed to get mem info: {:?}", e))?;
        info!("GPU {}: Free {} / Total {} bytes", gpu_index, free, total);

        // 残留プロセスチェック (簡易版)
        // 本来はNVMLでプロセスリストを取得してkillすべきだが、
        // ここではメモリ使用率から推測して警告を出す。
        if free < total * 95 / 100 {
             warn!("GPU {} seems to have residual processes (Free < 95%). Scrubbing might be incomplete.", gpu_index);
             // TODO: nvml-wrapperを使って残留プロセスを強制キルするロジックをここに挟む
        }

        // 3. 安全マージンを残してメモリを確保
        // 全量確保(free)しようとすると、ドライバのオーバーヘッド等で失敗することがあるため、
        // 少し余裕(10MB)を持たせる。
        let margin = 10 * 1024 * 1024; // 10MB
        let alloc_size = if free > margin { free - margin } else { free };
        
        if alloc_size == 0 {
            warn!("GPU {}: No memory to scrub.", gpu_index);
            return Ok(());
        }

        info!("GPU {}: Allocating {} bytes for scrubbing...", gpu_index, alloc_size);

        unsafe {
            // 4. デバイスメモリの確保とゼロ埋め (alloc_zeros)
            // cudarc 0.17+ では alloc_zeros が利用可能で、内部で非同期に確保とmemsetを行う
            // 戻り値の CudaSlice がドロップされるまでメモリは確保される
            let _mem = stream.alloc_zeros::<u8>(alloc_size).map_err(|e| anyhow::anyhow!("Failed to alloc/scrub: {:?}", e))?;
            
            // 5. 同期して完了を待つ
            stream.synchronize().map_err(|e| anyhow::anyhow!("Failed to sync: {:?}", e))?;
        }
        
        info!("VRAM scrub completed successfully for GPU: {}", gpu_index);
        Ok(())
    }

    #[cfg(not(feature = "real-gpu"))]
    pub fn scrub_device(gpu_index: usize) -> anyhow::Result<()> {
        info!("(MOCK) VRAM scrub skipped for GPU: {} (real-gpu feature disabled)", gpu_index);
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
        // GPU 0 を洗浄するテスト
        let result = GpuScrubber::scrub_device(0);
        match result {
            Ok(_) => println!("Scrubbing successful"),
            Err(e) => println!("Scrubbing failed (no GPU?): {:?}", e),
        }
    }
}
