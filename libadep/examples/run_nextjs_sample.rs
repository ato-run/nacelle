//! Example: Run the Next.js sample webcapsule
//!
//! This example demonstrates how to use libadep-runtime to execute
//! the Next.js sample capsule.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example run_nextjs_sample
//! ```
//!
//! Then visit http://localhost:3000 in your browser.
//! Press Ctrl+C to stop the capsule.

use libadep::{create_runtime, AdepContainerRuntime, CapsuleManifest};
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Gumball Webcapsule Runner Example");
    println!("=====================================\n");

    // Determine the path to the Next.js sample
    // From: gumball-adep/libadep/libadep -> gumball (project root)
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // gumball-adep/libadep
        .unwrap()
        .parent() // gumball-adep
        .unwrap()
        .parent() // gumball (project root)
        .unwrap()
        .to_path_buf();

    let capsule_path = workspace_root
        .join("gumball-webcapsule")
        .join("samples")
        .join("nextjs-sample");

    println!("📁 Capsule path: {}", capsule_path.display());

    // Check if the capsule exists
    if !capsule_path.exists() {
        eprintln!(
            "❌ Error: Capsule directory not found at {}",
            capsule_path.display()
        );
        eprintln!("\nPlease ensure the Next.js sample has been built:");
        eprintln!("  cd {}", capsule_path.parent().unwrap().display());
        eprintln!("  cd nextjs-sample");
        eprintln!("  npm install");
        eprintln!("  npm run build");
        std::process::exit(1);
    }

    let manifest_path = capsule_path.join("adep.json");
    if !manifest_path.exists() {
        eprintln!(
            "❌ Error: adep.json not found at {}",
            manifest_path.display()
        );
        std::process::exit(1);
    }

    // Create the runtime (platform-specific)
    let runtime = create_runtime();
    println!(
        "✅ Runtime created: {}",
        if cfg!(target_os = "linux") {
            "YoukiRuntime (Linux)"
        } else {
            "SimpleProcessRuntime (macOS/Windows)"
        }
    );

    // Load the manifest
    println!("📋 Loading manifest from: {}", manifest_path.display());
    let manifest = CapsuleManifest::load(&manifest_path)?;
    println!("   Name: {}", manifest.name);
    println!("   Version: {}", manifest.version);
    println!(
        "   Command: {} {}",
        manifest.entrypoint.command,
        manifest.entrypoint.args.join(" ")
    );

    if let Some(network) = &manifest.network {
        for port in &network.ports {
            println!(
                "   Port mapping: {} -> {}",
                port.container_port, port.host_port
            );
        }
    }

    // Run the capsule
    println!("\n🔄 Starting capsule...");
    let capsule_id = runtime.run(&manifest, &capsule_path).await?;
    println!("✅ Capsule started successfully!");
    println!("   Capsule ID: {}", capsule_id);

    // Give it a moment to start
    tokio::time::sleep(Duration::from_secs(2)).await;

    println!("\n🌐 Application running at: http://localhost:3000");
    println!("   Press Ctrl+C to stop the capsule\n");

    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;

    // Stop the capsule
    println!("\n🛑 Stopping capsule...");
    runtime.stop(&capsule_id).await?;
    println!("✅ Capsule stopped successfully");

    Ok(())
}
