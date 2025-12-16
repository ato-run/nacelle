use anyhow::{anyhow, bail, Result};
use dirs::home_dir;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use super::audit;
use super::engine::ResolvedEngine;
use crate::manifest::{Manifest, PlatformSpec, RuntimeSpec};

pub fn execute_container(
    engine: ResolvedEngine,
    manifest: &Manifest,
    runtime: &RuntimeSpec,
    platform: &PlatformSpec,
    root_path: &Path,
) -> Result<()> {
    // Determine container image (manifest overrides default)
    let image = if let Some(custom) = runtime.image.as_ref() {
        custom.clone()
    } else {
        match platform.language.as_str() {
            "python" => format!("python:{}-slim", platform.version),
            _ => bail!("Unsupported language: {}", platform.language),
        }
    };

    let explicit_host_port = std::env::var("ADEP_RUNTIME_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok());
    let container_port_env = std::env::var("ADEP_RUNTIME_CONTAINER_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok());
    let container_port = container_port_env.unwrap_or(8000);

    let port = if let Some(host_port) = explicit_host_port {
        ensure_port_available(host_port)?;
        host_port
    } else {
        find_available_port(8000, 8010)
            .ok_or_else(|| anyhow!("No available ports in range 8000-8010"))?
    };

    // Start audit proxy if needed
    let audit_guard = if manifest.network.http_proxy_dev {
        let app_name = manifest
            .publish_info
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        Some(audit::spawn(
            app_name.to_string(),
            format!("{}:{}", platform.language, platform.version),
        )?)
    } else {
        None
    };

    // Prepare cache directory
    let home = home_dir().ok_or_else(|| crate::error::home_dir_unavailable())?;
    let cache_dir = home.join(".adep/pip-cache");
    fs::create_dir_all(&cache_dir)?;

    // Build container command
    let cmd_name = engine.command_name();
    let mut cmd = Command::new(cmd_name);

    cmd.arg("run")
        .arg("--rm")
        .arg("--name")
        .arg(format!("adep-{}", manifest.id))
        .arg("-p")
        .arg(format!("{}:{}", port, container_port))
        .arg("-v")
        .arg(format!("{}:/app:ro", root_path.display()))
        .arg("-v")
        .arg(format!("{}:/pip-cache", cache_dir.display()))
        .arg("-w")
        .arg("/app");

    // Environment variables (pass to container with -e flag)
    cmd.arg("-e").arg("ADEP_APP_ROOT=/app");
    cmd.arg("-e")
        .arg(format!("ADEP_ENTRY={}", platform.entry.display()));
    cmd.arg("-e").arg("PIP_CACHE_DIR=/pip-cache");

    // Pass through selected environment variables from host
    for (key, value) in std::env::vars() {
        if key.starts_with("GUMBALL_") || key.starts_with("ADEP_DEP_") {
            cmd.arg("-e").arg(format!("{}={}", key, value));
        }
    }

    if let Some(deps) = &platform.dependencies {
        cmd.arg("-e")
            .arg(format!("ADEP_DEP_PATH={}", deps.display()));
    }

    if let Some(wheels) = &platform.wheels {
        cmd.arg("-e")
            .arg(format!("ADEP_WHEELS={}", wheels.display()));
    }

    // Set proxy if audit is enabled (コンテナから到達可能なURL)
    if let Some(ref guard) = audit_guard {
        let container_proxy_url = guard.proxy_url_for_container();
        cmd.arg("-e")
            .arg(format!("HTTP_PROXY={}", container_proxy_url));
        cmd.arg("-e")
            .arg(format!("HTTPS_PROXY={}", container_proxy_url));
    }

    // Add --add-host for Linux audit proxy
    #[cfg(target_os = "linux")]
    if audit_guard.is_some() {
        cmd.arg("--add-host")
            .arg("host.containers.internal:host-gateway");
    }

    // macOS/Windows でも --add-host を追加（Podman の場合）
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    if audit_guard.is_some() && engine == ResolvedEngine::Podman {
        cmd.arg("--add-host")
            .arg("host.docker.internal:host-gateway");
    }

    // Use bootstrap.py as entrypoint
    let bootstrap_py = include_str!("bootstrap.py");
    cmd.arg(&image).arg("sh").arg("-c").arg(format!(
        "cat > /tmp/bootstrap.py << 'BOOTSTRAP_EOF'\n{}\nBOOTSTRAP_EOF\npython3 /tmp/bootstrap.py",
        bootstrap_py
    ));

    // Print startup message
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🚀 ADEP Runtime Server");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    if let Some(publish_info) = &manifest.publish_info {
        if let Some(name) = &publish_info.name {
            println!("  App:     {}", name);
        }
    }
    println!("  Version: {}", manifest.version.number);
    println!("  Image:   {}", image);
    println!("  Engine:  {}", cmd_name);
    println!(
        "  URL:     http://localhost:{} (container port {})",
        port, container_port
    );
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if !manifest.network.egress_allow.is_empty() {
        println!("📋 Allowed egress:");
        for url in &manifest.network.egress_allow {
            println!("   - {}", url);
        }
    }

    println!("Press Ctrl+C to stop\n");

    // Execute
    let mut child = cmd
        .stdin(Stdio::null())
        .spawn()
        .map_err(|err| anyhow!("Failed to start container: {}", err))?;

    // Registry に登録
    let mut registry = super::registry::AdepRegistry::load()?;
    let app_name = manifest
        .publish_info
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unnamed".to_string());

    let running_adep = super::registry::RunningAdep {
        name: app_name.clone(),
        family_id: manifest.family_id.to_string(),
        version: manifest.version.number.clone(),
        pid: child.id(),
        ports: {
            let mut ports = std::collections::HashMap::new();
            ports.insert("primary".to_string(), port);
            ports
        },
        started_at: chrono::Utc::now().to_rfc3339(),
        manifest_path: root_path.join("manifest.json"),
    };

    registry.register(running_adep);
    registry.save()?;
    eprintln!(
        "✓ Registered in .adep/local-registry.json (PID: {}, port: {})",
        child.id(),
        port
    );

    // Ctrl+C ハンドラ（クリーンアップ用）
    let family_id_clone = manifest.family_id.to_string();
    ctrlc::set_handler(move || {
        eprintln!("\n🛑 Shutting down...");
        // クリーンアップ
        if let Ok(mut reg) = super::registry::AdepRegistry::load() {
            reg.unregister(&family_id_clone);
            let _ = reg.save();
            eprintln!("✓ Unregistered from .adep/local-registry.json");
        }
        std::process::exit(0);
    })
    .ok();

    let status = child.wait()?;

    // 正常終了時のクリーンアップ
    let mut registry = super::registry::AdepRegistry::load()?;
    registry.unregister(&manifest.family_id.to_string());
    registry.save()?;
    eprintln!("✓ Unregistered from .adep/local-registry.json");

    // Clean up audit guard
    drop(audit_guard);

    if !status.success() {
        bail!(
            "Container exited with error code: {}",
            status.code().unwrap_or(-1)
        );
    }

    Ok(())
}

fn find_available_port(start: u16, end: u16) -> Option<u16> {
    use std::net::TcpListener;

    for port in start..=end {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Some(port);
        }
    }
    None
}

fn ensure_port_available(port: u16) -> Result<()> {
    use std::net::TcpListener;
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(_) => Err(anyhow!("Port {} is already in use", port)),
    }
}
