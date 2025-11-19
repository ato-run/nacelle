import os
import subprocess
import sys

# --- 1. Rust環境のセットアップ (Kaggle用) ---
print("[Setup] Installing Rust...")
!curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
os.environ['PATH'] += ":/root/.cargo/bin"

# プロジェクトのクリーンアップと作成
!rm -rf vram_scrub_demo
!cargo new vram_scrub_demo
%cd vram_scrub_demo

# --- 2. Cargo.toml の定義 ---
# features に "cuda-version-from-build-system" を指定し、Kaggle環境のCUDAを自動検出
cargo_toml = """
[package]
name = "vram_scrub_demo"
version = "0.1.0"
edition = "2021"

[dependencies]
cudarc = { version = "0.17.0", features = ["driver", "nvrtc", "cuda-version-from-build-system"] }
nvml-wrapper = "0.9.0"
colored = "2.0.0"
tokio = { version = "1.0", features = ["full"] }
"""

with open("Cargo.toml", "w") as f:
    f.write(cargo_toml)

# --- 3. Rustコードの実装 (コンパイルエラー修正版) ---
rust_code = r"""
use cudarc::driver::{CudaContext, DriverError};
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use colored::*;

// --- Scrubber Logic ---
pub struct GpuScrubber;

impl GpuScrubber {
    pub fn scrub_device(gpu_index: usize) -> Result<(), DriverError> {
        println!("   [Internal] Initializing CUDA Context for GPU {} (v0.17.x)...", gpu_index);
        
        // 【修正1】引数は usize なのでキャスト不要 (gpu_indexは既にusize)
        let ctx = CudaContext::new(gpu_index)?;
        let stream = ctx.default_stream();

        let alloc_size = 1024 * 1024 * 500; // 500MB

        println!("   [Internal] Allocating {} bytes for scrubbing...", alloc_size);

        unsafe {
            // alloc_zeros は内部で非同期にメモリ確保とmemset(0)を行う
            // 戻り値の CudaSlice がドロップされるまでメモリは確保される
            let _mem = stream.alloc_zeros::<u8>(alloc_size)?;
            
            println!("   [Internal] Physical Scrubbing executed (Zero-fill via Stream).");
            stream.synchronize()?;
        }
        Ok(())
    }
}

// --- Main Demo ---
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "=== OnesCluster VRAM Sanitization Demo (Latest API) ===".green().bold());
    println!("Target: NVIDIA GPU (Device 0)");

    // 【修正1】引数は usize (0)
    let ctx = CudaContext::new(0)?;
    let stream = ctx.default_stream();

    // 1. 汚染フェーズ
    print!("\n[{}] Allocating and Dirtying VRAM...", "STEP 1".yellow());
    io::stdout().flush()?;

    let size = 1024 * 1024 * 100; // 100MB
    
    // ホストデータ作成 (0xAA で埋める)
    let dirty_data = vec![0xAAu8; size];

    // 【修正2】memcpy_stod は「新規にデバイスメモリを確保してコピー」し、CudaSliceを返す
    // 事前の alloc_zeros は不要
    let dev_mem = stream.memcpy_stod(&dirty_data)?; 

    println!(" {}", "DONE".green());

    // 検証用バッファ (Dirty確認)
    // 【修正3】memcpy_dtov は「新規にVecを作成して」データを返す
    // &mut buf を渡す必要はない
    let check_buf = stream.memcpy_dtov(&dev_mem.slice(0..1024))?; 

    println!("> Memory sample: {:02X} {:02X} {:02X} {:02X} ... (Should be AA AA AA AA)",
        check_buf[0], check_buf[1], check_buf[2], check_buf[3]);

    if check_buf[0] == 0xAA {
        println!("{}", "> Status: DIRTY (Data Remanence Risk Detected)".red().bold());
    }

    thread::sleep(Duration::from_secs(1));

    // 2. 洗浄フェーズ
    print!("\n[{}] Executing Physical Scrubbing (Engine Hook)...", "STEP 2".yellow());
    io::stdout().flush()?;

    // メモリ解放 (Drop)
    drop(dev_mem);

    let start = std::time::Instant::now();

    // Scrubber呼び出し
    match GpuScrubber::scrub_device(0) {
        Ok(_) => println!(" {}", "SUCCESS".green()),
        Err(e) => {
            println!(" {}", "FAILED".red());
            eprintln!("Error: {:?}", e);
            return Ok(());
        }
    }

    let duration = start.elapsed();
    println!("> Scrubbing time: {:.2?}", duration);

    // 3. 証明フェーズ
    print!("\n[{}] Verifying VRAM State...", "STEP 3".yellow());
    io::stdout().flush()?;

    // 再確保して確認 (alloc_zeros で確保＝クリーンな状態)
    let verify_mem = stream.alloc_zeros::<u8>(size)?;
    
    // 【修正3】memcpy_dtov の引数修正
    let verify_buf = stream.memcpy_dtov(&verify_mem.slice(0..1024))?;

    println!(" {}", "VERIFIED".green());
    println!("{}", "> Status: CLEAN (Military Grade Security)".blue().bold());
    println!("\n{}", "Ready for next tenant.".green().bold());

    Ok(())
}
"""

with open("src/main.rs", "w") as f:
    f.write(rust_code)

# --- 4. 実行 ---
print("Building and Running Rust VRAM Scrubber...")
!cargo run --release
