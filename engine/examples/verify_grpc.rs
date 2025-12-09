use capsuled_engine::proto::onescluster::coordinator::v1::engine_service_client::EngineServiceClient;
use capsuled_engine::proto::onescluster::coordinator::v1::{
    ExecuteCapsuleRequest, TerminateCapsuleRequest,
};
use colored::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "http://127.0.0.1:50051";
    println!("{}", format!("🔌 Connecting to Engine at {}", addr).cyan());

    let mut client = EngineServiceClient::connect(addr).await?;

    // Step 1: Get Hardware Info
    println!("\n{}", "📊 Step 1: GetHardwareInfo".bold());
    let request = tonic::Request::new(());
    let response = client.get_hardware_info(request).await?;
    let hardware_info = response.into_inner();

    if !hardware_info.gpus.is_empty() {
        let gpu = &hardware_info.gpus[0];
        let total_vram = gpu.vram_total_bytes;

        if total_vram > 0 {
            let total_gb = total_vram as f64 / 1024.0 / 1024.0 / 1024.0;
            println!(
                "{}",
                format!(
                    "[SUCCESS] GPU detected: {} - {:.2} GB VRAM total",
                    gpu.name, total_gb
                )
                .green()
            );
        } else {
            println!(
                "{}",
                "[WARNING] VRAM is 0 - GPU detection may have failed"
                    .yellow()
            );
        }
    } else {
        println!("{}", "[WARNING] No GPUs detected".yellow());
    }

    // Step 2: Build test manifest using mock_runtime (direct path)
    println!("\n{}", "📝 Step 2: Building test manifest".bold());
    
    // Get absolute path to mock_runtime.sh in engine directory
    let mock_runtime_path = std::env::current_dir()
        .expect("Failed to get current dir")
        .join("mock_runtime.sh");
    
    if !mock_runtime_path.exists() {
        println!("{}", format!("[ERROR] mock_runtime.sh not found at {}", mock_runtime_path.display()).red());
        return Ok(());
    }
    
    // Build JSON manifest matching AdepManifest structure
    // Using direct path for runtime - NativeRuntime supports this
    let test_manifest_json = serde_json::json!({
        "name": "test-verification-capsule",
        "metadata": {},
        "scheduling": {
            "gpu": {
                "vram_min_gb": 0
            }
        },
        "compute": {
            "image": "",
            "args": [],
            "env": [],
            "native": {
                "runtime": mock_runtime_path.to_str().unwrap(),
                "args": ["--pid-file", "/tmp/test-capsule.pid"]
            }
        },
        "volumes": []
    });

    println!("{}", format!("[INFO] Using direct path: {}", mock_runtime_path.display()).dimmed());

    // Step 3: ExecuteCapsule
    println!("\n{}", "🚀 Step 3: ExecuteCapsule".bold());
    let exec_request = ExecuteCapsuleRequest {
        capsule_id: "test-verification-capsule".to_string(),
        runtime: None, // RuntimeSpec is optional
        env: std::collections::HashMap::new(),
        args: vec![],
        port: 0,
        manifest: Some(
            capsuled_engine::proto::onescluster::coordinator::v1::execute_capsule_request::Manifest::AdepJson(
                test_manifest_json.to_string().into_bytes(),
            ),
        ),
    };

    let exec_response = client.execute_capsule(exec_request).await?;
    let exec_result = exec_response.into_inner();

    let pid = exec_result.pid;
    let actual_port = exec_result.actual_port;

    if pid > 0 {
        println!(
            "{}",
            format!(
                "[SUCCESS] Capsule started (PID: {}, Port: {})",
                pid, actual_port
            )
            .green()
        );
    } else {
        println!(
            "{}",
            format!(
                "[ERROR] Capsule failed to start (PID: {})",
                pid
            )
            .red()
        );
        return Ok(());
    }

    // Step 4: Verify process exists
    println!("\n{}", "🔍 Step 4: Verifying process exists".bold());
    
    use sysinfo::{System, Pid};
    let mut sys = System::new_all();
    sys.refresh_all();

    let process_pid = Pid::from_u32(pid as u32);
    if let Some(process) = sys.process(process_pid) {
        println!(
            "{}",
            format!(
                "[SUCCESS] Process verified running (PID: {}, Name: {:?})",
                pid,
                process.name()
            )
            .green()
        );
    } else {
        println!(
            "{}",
            format!("[ERROR] Process with PID {} not found", pid).red()
        );
        return Ok(());
    }

    // Sleep for a moment to ensure process is stable
    println!("{}", "[INFO] Waiting 2 seconds...".dimmed());
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Step 5: TerminateCapsule
    println!("\n{}", "🛑 Step 5: TerminateCapsule".bold());
    let term_request = TerminateCapsuleRequest {
        capsule_id: "test-verification-capsule".to_string(),
        signal: 15, // SIGTERM
        timeout_seconds: 10,
    };

    let term_response = client.terminate_capsule(term_request).await?;
    let term_result = term_response.into_inner();

    if term_result.success {
        println!("{}", format!("[SUCCESS] Capsule terminated (exit_code: {})", term_result.exit_code).green());
    } else {
        println!(
            "{}",
            format!("[ERROR] Termination failed (exit_code: {})", term_result.exit_code).red()
        );
        return Ok(());
    }

    // Step 6: Verify process is gone
    println!("\n{}", "🔍 Step 6: Verifying process stopped".bold());
    
    // Sleep briefly to allow termination to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    sys.refresh_all();
    if sys.process(process_pid).is_none() {
        println!(
            "{}",
            format!("[SUCCESS] Process verified stopped (PID: {} not found)", pid).green()
        );
    } else {
        println!(
            "{}",
            format!(
                "[WARNING] Process with PID {} still exists (may take time to terminate)",
                pid
            )
            .yellow()
        );
    }

    println!("\n{}", "✅ E2E Verification Complete!".bold().green());

    Ok(())
}
