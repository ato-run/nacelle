use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;
use tiny_http::{Header, Response, Server};

use crate::manifest::{EgressMode, Manifest};
use crate::runtime;

#[derive(Args, Debug, Clone)]
pub struct RunArgs {
    /// Package root directory
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// HTTP server port
    #[arg(long, default_value = "3000")]
    pub port: u16,
    /// Skip signature verification (dev mode only)
    #[arg(long)]
    pub skip_verify: bool,
}

pub fn run(args: &RunArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        cwd.join(&args.root)
    };

    // 1. Verify package integrity
    if !args.skip_verify {
        println!("Verifying package integrity...");
        super::verify::run(&super::verify::VerifyArgs {
            root: root.clone(),
            skip_signature: false,
        })?;
        println!("✓ Verification passed\n");
    } else {
        println!("⚠️  Warning: Running without verification (dev mode)\n");
    }

    // 2. Load manifest
    let manifest_path = root.join("manifest.json");
    let manifest = Manifest::load(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;

    // 2.5. Dev mode チェック
    check_dev_mode(&manifest)?;

    // 2.6. 依存関係検証 + 環境変数注入
    check_dependencies(&manifest)?;

    // 3. Runtime実行（コンテナ）
    if manifest.runtime_profile().is_some() {
        return runtime::execute_manifest(&manifest, &root);
    }

    // 4. 静的アプリの場合は既存のサーバーを起動
    // 4.1. Registry に登録
    let mut registry = crate::runtime::AdepRegistry::load()?;
    let app_name = manifest
        .publish_info
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unnamed".to_string());

    let running_adep = crate::runtime::registry::RunningAdep {
        name: app_name.clone(),
        family_id: manifest.family_id.to_string(),
        version: manifest.version.number.clone(),
        pid: std::process::id(),
        ports: {
            let mut ports = std::collections::HashMap::new();
            ports.insert("primary".to_string(), args.port);
            ports
        },
        started_at: chrono::Utc::now().to_rfc3339(),
        manifest_path: manifest_path.clone(),
    };

    registry.register(running_adep);
    registry.save()?;

    eprintln!(
        "✓ Registered in .adep/local-registry.json (port: {})",
        args.port
    );

    // 4.2. Generate CSP
    let csp = build_csp(&manifest);

    // 5. Start HTTP server
    // 環境変数 ADEP_BIND_HOST を優先、デフォルトは 127.0.0.1
    // デベロップメントモードでは 0.0.0.0 も許可
    let bind_host = if std::env::var("ADEP_ALLOW_DEV_MODE").ok().as_deref() == Some("1") {
        std::env::var("ADEP_BIND_HOST").unwrap_or_else(|_| "0.0.0.0".to_string())
    } else {
        "127.0.0.1".to_string()
    };

    if bind_host == "0.0.0.0" {
        eprintln!("⚠️  WARNING: Binding to 0.0.0.0 (all interfaces)");
        eprintln!("⚠️  This allows connections from other devices on the network");
        eprintln!();
    }

    let addr = format!("{}:{}", bind_host, args.port);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🚀 ADEP Runtime Server");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "  App:     {}",
        manifest
            .publish_info
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .unwrap_or(&"Unnamed App".to_string())
    );
    println!("  Version: {}", manifest.version.number);
    println!("  URL:     http://{}", addr);
    println!("  Root:    {}", root.display());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("\n📋 Content Security Policy:");
    println!("  {}\n", csp);
    println!("Press Ctrl+C to stop\n");

    let server = Server::http(&addr)
        .map_err(|e| anyhow::anyhow!("failed to start server on {}: {}", addr, e))?;

    let dist = root.join("dist");
    if !dist.exists() {
        bail!("dist directory not found at {}", dist.display());
    }

    // Ctrl+C ハンドラをセットアップ（クリーンアップ用）
    let family_id_clone = manifest.family_id.to_string();
    ctrlc::set_handler(move || {
        eprintln!("\n🛑 Shutting down...");
        // クリーンアップ
        if let Ok(mut registry) = crate::runtime::AdepRegistry::load() {
            registry.unregister(&family_id_clone);
            let _ = registry.save();
            eprintln!("✓ Unregistered from .adep/local-registry.json");
        }
        std::process::exit(0);
    })
    .context("Failed to set Ctrl+C handler")?;

    for request in server.incoming_requests() {
        let url_path = request.url().trim_start_matches('/');

        // Sanitize path to prevent directory traversal
        let safe_path = match sanitize_path(url_path) {
            Ok(p) => p,
            Err(_e) => {
                eprintln!("⚠️  Path traversal attempt blocked: {}", url_path);
                let response = Response::from_string("400 Bad Request").with_status_code(400);
                let _ = request.respond(response);
                continue;
            }
        };

        let file_path = if safe_path.is_empty() {
            dist.join("index.html")
        } else {
            dist.join(safe_path)
        };

        // Ensure file is within dist directory
        let canonical_file = match file_path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                let response = Response::from_string("404 Not Found").with_status_code(404);
                let _ = request.respond(response);
                continue;
            }
        };

        let canonical_dist = match dist.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("❌ Failed to resolve dist directory");
                let response =
                    Response::from_string("500 Internal Server Error").with_status_code(500);
                let _ = request.respond(response);
                continue;
            }
        };

        if !canonical_file.starts_with(&canonical_dist) {
            eprintln!("⚠️  Access outside dist directory blocked: {}", url_path);
            let response = Response::from_string("403 Forbidden").with_status_code(403);
            let _ = request.respond(response);
            continue;
        }

        let response = if canonical_file.is_file() {
            match std::fs::read(&canonical_file) {
                Ok(content) => {
                    let mime = guess_mime(&canonical_file);
                    println!("→ {} [{}]", url_path, mime);

                    let mut resp = Response::from_data(content);

                    // Add CSP header
                    if let Ok(header) =
                        Header::from_bytes(&b"Content-Security-Policy"[..], csp.as_bytes())
                    {
                        resp = resp.with_header(header);
                    }

                    // Add Content-Type header
                    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()) {
                        resp = resp.with_header(header);
                    }

                    // Add CORS headers for development
                    if let Ok(header) =
                        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], b"*")
                    {
                        resp = resp.with_header(header);
                    }

                    resp
                }
                Err(e) => {
                    eprintln!("❌ Failed to read file: {}", e);
                    Response::from_string("500 Internal Server Error").with_status_code(500)
                }
            }
        } else {
            println!("→ {} [404]", url_path);
            Response::from_string("404 Not Found").with_status_code(404)
        };

        if let Err(e) = request.respond(response) {
            eprintln!("❌ Failed to send response: {}", e);
        }
    }

    Ok(())
}

fn build_csp(manifest: &Manifest) -> String {
    let connect_src = if manifest.network.egress_allow.is_empty() {
        "'self'".to_string()
    } else {
        format!("'self' {}", manifest.network.egress_allow.join(" "))
    };

    // Relaxed CSP for development (allows inline event handlers)
    // Production apps should migrate to external event handlers
    // Note: unsafe-inline allows onclick="..." attributes
    format!(
        "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-hashes'; connect-src {}; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:",
        connect_src
    )
}

fn sanitize_path(path: &str) -> Result<String> {
    let path_obj = Path::new(path);

    for component in path_obj.components() {
        match component {
            Component::Normal(_) => continue,
            Component::RootDir => continue,
            Component::CurDir => continue,
            Component::ParentDir => {
                bail!("path contains parent directory reference");
            }
            Component::Prefix(_) => {
                bail!("path contains prefix");
            }
        }
    }

    Ok(path.to_string())
}

fn guess_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") | Some("cjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("wasm") => "application/wasm",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("eot") => "application/vnd.ms-fontobject",
        Some("xml") => "application/xml",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Dev mode がユーザーによって明示的に許可されているか
fn is_dev_mode_allowed() -> bool {
    std::env::var("ADEP_ALLOW_DEV_MODE").ok().as_deref() == Some("1")
}

/// Dev mode のチェック
fn check_dev_mode(manifest: &Manifest) -> Result<()> {
    if manifest.network.egress_mode == Some(EgressMode::Dev) {
        if !is_dev_mode_allowed() {
            bail!(
                "ADEP-DEV-MODE-BLOCKED: \n\
                egress_mode: \"dev\" requires explicit permission.\n\
                \n\
                To enable (development only):\n\
                export ADEP_ALLOW_DEV_MODE=1\n\
                \n\
                WARNING: Never set this in production!\n\
                This allows localhost communication which bypasses ADEP's security model."
            );
        }

        eprintln!("⚠️  WARNING: Development mode enabled");
        eprintln!("⚠️  localhost communication is allowed");
        eprintln!("⚠️  ADEP_ALLOW_DEV_MODE=1 detected");
        eprintln!();

        // 使用記録（監査用）
        record_dev_mode_usage(manifest)?;
    }

    Ok(())
}

/// Dev mode 使用記録
fn record_dev_mode_usage(manifest: &Manifest) -> Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    // allow override via env var (useful in sandboxed envs)
    let log_path = if let Ok(custom) = std::env::var("ADEP_DEV_MODE_LOG") {
        PathBuf::from(custom)
    } else {
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("HOME not found"))?
            .join(".adep")
            .join("dev_mode_usage.log")
    };

    // ディレクトリ作成
    if let Some(parent) = log_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!(
                "⚠️  Dev mode log directory not writable ({}): {}",
                parent.display(),
                e
            );
            return Ok(());
        }
    }

    let entry = format!(
        "{} - {} ({})\n",
        chrono::Utc::now().to_rfc3339(),
        manifest.family_id,
        manifest
            .publish_info
            .as_ref()
            .and_then(|p| p.name.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("unnamed")
    );

    let mut file = match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "⚠️  Dev mode log not recorded ({}): {}",
                log_path.display(),
                e
            );
            return Ok(());
        }
    };

    if let Err(e) = file.write_all(entry.as_bytes()) {
        eprintln!(
            "⚠️  Dev mode log write failed ({}): {}",
            log_path.display(),
            e
        );
    }

    Ok(())
}

/// 依存関係検証 + 環境変数注入
fn check_dependencies(manifest: &Manifest) -> Result<()> {
    let Some(deps) = &manifest.dependencies else {
        return Ok(());
    };

    if deps.adep.is_empty() {
        return Ok(());
    }

    let registry = crate::runtime::AdepRegistry::load()?;

    for dep in &deps.adep {
        // 1. ADEPが起動しているか（必須チェック）
        let running = registry.find_by_family_id(&dep.family_id).ok_or_else(|| {
            anyhow::anyhow!(
                "ADEP-DEP-NOT-RUNNING: Required ADEP '{}' is not running.\n\
                \n\
                Start it first:\n\
                cd ../{} && adep run",
                dep.name,
                dep.name
            )
        })?;

        // 2. 実際のポートを取得
        let actual_port = running
            .ports
            .get("primary")
            .ok_or_else(|| anyhow::anyhow!("ADEP '{}' has no primary port defined", dep.name))?;

        // 3. ポート検証（警告のみ）
        if let Some(expected_port) = dep.port {
            if *actual_port != expected_port {
                eprintln!("⚠️  WARNING: ADEP '{}' port mismatch", dep.name);
                eprintln!("    Expected: {}, Running: {}", expected_port, actual_port);
                eprintln!("    Communication may fail if hardcoded");
                eprintln!();
            }
        }

        // 4. 環境変数注入（新規）
        let env_port_key = format!(
            "ADEP_DEP_{}_PORT",
            dep.name.to_uppercase().replace('-', "_")
        );
        let env_url_key = format!("ADEP_DEP_{}_URL", dep.name.to_uppercase().replace('-', "_"));

        std::env::set_var(&env_port_key, actual_port.to_string());
        std::env::set_var(&env_url_key, format!("http://localhost:{}", actual_port));

        eprintln!("  ✓ Injected: {}={}", env_port_key, actual_port);
        eprintln!(
            "  ✓ Injected: {}=http://localhost:{}",
            env_url_key, actual_port
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_dev_mode_allowed() {
        // ADEP_ALLOW_DEV_MODE=1 のみ許可
        std::env::remove_var("ADEP_ALLOW_DEV_MODE");
        assert!(!is_dev_mode_allowed());

        std::env::set_var("ADEP_ALLOW_DEV_MODE", "0");
        assert!(!is_dev_mode_allowed());

        std::env::set_var("ADEP_ALLOW_DEV_MODE", "1");
        assert!(is_dev_mode_allowed());

        std::env::remove_var("ADEP_ALLOW_DEV_MODE");
    }
}
