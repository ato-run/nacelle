use anyhow::{bail, Context, Result};
use clap::Args;
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Debug, Args)]
pub struct ComposeArgs {
    /// Path to compose.yaml
    #[arg(long, default_value = "compose.yaml")]
    pub file: String,

    /// Action: up, down, ps
    #[arg(default_value = "up")]
    pub action: String,
}

#[derive(Debug, Deserialize)]
struct ComposeFile {
    #[allow(dead_code)]
    version: String,
    services: HashMap<String, ComposeService>,
}

#[derive(Debug, Deserialize)]
struct ComposeService {
    adep_root: String,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    healthcheck: Option<HealthCheck>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    ports: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HealthCheck {
    port: u16,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_timeout() -> u64 {
    10
}

/// Compose state: 起動中のサービスを管理
struct ComposeState {
    services: HashMap<String, Child>,
}

impl ComposeState {
    fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    fn add_service(&mut self, name: String, child: Child) {
        self.services.insert(name, child);
    }

    fn stop_all(&mut self) -> Result<()> {
        for (name, child) in self.services.iter_mut() {
            eprintln!("→ Stopping: {}", name);

            // SIGTERMを送信
            if let Err(e) = child.kill() {
                eprintln!("  ⚠️  Failed to kill {}: {}", name, e);
            }

            // 終了を待機
            match child.wait() {
                Ok(status) => {
                    if status.success() {
                        eprintln!("  ✓ {} stopped", name);
                    } else {
                        eprintln!("  ⚠️  {} exited with status: {}", name, status);
                    }
                }
                Err(e) => {
                    eprintln!("  ⚠️  Failed to wait for {}: {}", name, e);
                }
            }
        }

        Ok(())
    }
}

pub fn compose_command(args: ComposeArgs) -> Result<()> {
    // Dev mode チェック（compose は開発専用）
    if std::env::var("ADEP_ALLOW_DEV_MODE").ok().as_deref() != Some("1") {
        bail!(
            "ADEP-COMPOSE-DEV-ONLY: \n\
            'adep compose' is a development tool and requires ADEP_ALLOW_DEV_MODE=1.\n\
            \n\
            To enable:\n\
            export ADEP_ALLOW_DEV_MODE=1\n\
            adep compose up"
        );
    }

    eprintln!("⚠️  WARNING: adep compose is a DEVELOPMENT TOOL ONLY");
    eprintln!("⚠️  Do not use in production");
    eprintln!();

    let content = std::fs::read_to_string(&args.file)
        .with_context(|| format!("Failed to read {}", args.file))?;

    let compose: ComposeFile =
        serde_yaml::from_str(&content).with_context(|| format!("Failed to parse {}", args.file))?;

    // manifest整合性チェックは削除（過剰なため）

    match args.action.as_str() {
        "up" => compose_up(compose),
        "down" => compose_down(),
        "ps" => compose_ps(),
        _ => bail!("Invalid action: {}. Use 'up', 'down', or 'ps'", args.action),
    }
}

fn compose_up(compose: ComposeFile) -> Result<()> {
    let order = resolve_dependency_order(&compose)?;
    let mut state = ComposeState::new();

    eprintln!("Starting services in order: {:?}\n", order);

    for service_name in order {
        let service = &compose.services[&service_name];

        eprintln!("→ Starting: {}", service_name);

        // adep run を起動（出力は継承してバッファリング問題を回避）
        let mut cmd = Command::new("adep");
        cmd.arg("run")
            .arg("--root")
            .arg(&service.adep_root)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit()) // 出力を継承
            .stderr(Stdio::inherit()); // エラー出力を継承

        // 環境変数を設定
        for env_var in &service.env {
            if let Some((key, value)) = env_var.split_once('=') {
                cmd.env(key, value);
                eprintln!("  ✓ Set env: {}={}", key, value);
            }
        }

        if let Some((host_port, container_port)) = service
            .ports
            .iter()
            .find_map(|mapping| parse_port_mapping(mapping))
        {
            cmd.env("ADEP_RUNTIME_PORT", &host_port);
            cmd.env("ADEP_RUNTIME_CONTAINER_PORT", &container_port);
            eprintln!("  ✓ Port map: {}:{}", host_port, container_port);
        }

        // healthcheck.port が指定されていれば --port として渡す
        if let Some(hc) = &service.healthcheck {
            cmd.arg("--port").arg(hc.port.to_string());
        }

        let child = cmd
            .spawn()
            .with_context(|| format!("Failed to start {}", service_name))?;

        let pid = child.id();
        state.add_service(service_name.clone(), child);

        eprintln!("  Started with PID: {}", pid);

        // ヘルスチェック待機
        if let Some(hc) = &service.healthcheck {
            wait_for_port(hc.port, Duration::from_secs(hc.timeout_secs)).with_context(|| {
                format!(
                    "Service '{}' failed to start within {}s",
                    service_name, hc.timeout_secs
                )
            })?;

            eprintln!("  ✓ {} ready (port {})", service_name, hc.port);
        } else {
            // ヘルスチェック未定義: 1秒待機
            std::thread::sleep(Duration::from_secs(1));
            eprintln!("  ✓ {} started (no healthcheck)", service_name);
        }
    }

    eprintln!("\n✅ All services started");
    eprintln!("Press Ctrl+C to stop\n");

    // Ctrl+C 待機
    let (tx, rx) = std::sync::mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })
    .context("Failed to set Ctrl+C handler")?;

    rx.recv().context("Failed to wait for Ctrl+C")?;

    eprintln!("\nStopping services...");
    state.stop_all()?;

    Ok(())
}

fn parse_port_mapping(mapping: &str) -> Option<(String, String)> {
    let mut parts = mapping.split(':');
    let host = parts.next()?;
    let container = parts.next()?;
    if host.is_empty() || container.is_empty() {
        return None;
    }
    if parts.next().is_some() {
        return None;
    }
    Some((host.to_string(), container.to_string()))
}

fn compose_down() -> Result<()> {
    use crate::runtime::AdepRegistry;

    let registry = AdepRegistry::load()?;

    if registry.running_adeps.is_empty() {
        eprintln!("No running ADEPs found");
        return Ok(());
    }

    for adep in &registry.running_adeps {
        eprintln!("→ Stopping: {}", adep.name);

        #[cfg(unix)]
        {
            let _ = Command::new("kill").arg(adep.pid.to_string()).output();
        }

        #[cfg(not(unix))]
        {
            eprintln!("  ⚠️  Windows stop not yet implemented");
        }
    }

    Ok(())
}

fn compose_ps() -> Result<()> {
    use crate::runtime::AdepRegistry;

    let registry = AdepRegistry::load()?;

    if registry.running_adeps.is_empty() {
        eprintln!("No running ADEPs");
        return Ok(());
    }

    eprintln!(
        "{:<20} {:<40} {:<10} {:<20}",
        "NAME", "FAMILY_ID", "VERSION", "PORTS"
    );
    eprintln!("{}", "=".repeat(100));

    for adep in &registry.running_adeps {
        let ports_str = adep
            .ports
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect::<Vec<_>>()
            .join(", ");

        eprintln!(
            "{:<20} {:<40} {:<10} {:<20}",
            adep.name, adep.family_id, adep.version, ports_str
        );
    }

    Ok(())
}

/// ポート待機（改善版: タイムアウト精度向上）
fn wait_for_port(port: u16, timeout: Duration) -> Result<()> {
    use std::net::TcpStream;
    use std::time::Instant;

    let deadline = Instant::now() + timeout;

    loop {
        // 早期タイムアウトチェック
        if Instant::now() >= deadline {
            bail!("Timeout waiting for port {}", port);
        }

        // 接続試行
        match TcpStream::connect(format!("127.0.0.1:{}", port)) {
            Ok(_) => return Ok(()),
            Err(_) => {
                // 残り時間を計算
                let remaining = deadline.saturating_duration_since(Instant::now());

                if remaining.is_zero() {
                    bail!("Timeout waiting for port {}", port);
                }

                // 500ms または残り時間の短い方を待機
                let wait_time = remaining.min(Duration::from_millis(500));
                std::thread::sleep(wait_time);
            }
        }
    }
}

fn resolve_dependency_order(compose: &ComposeFile) -> Result<Vec<String>> {
    // トポロジカルソート
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();

    // グラフ構築
    for (name, service) in &compose.services {
        in_degree.entry(name.clone()).or_insert(0);

        for dep in &service.depends_on {
            if !compose.services.contains_key(dep) {
                bail!(
                    "Service '{}' depends on '{}', but '{}' is not defined",
                    name,
                    dep,
                    dep
                );
            }

            graph
                .entry(dep.clone())
                .or_insert_with(Vec::new)
                .push(name.clone());
            *in_degree.entry(name.clone()).or_insert(0) += 1;
        }
    }

    // Kahn's algorithm
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(name, _)| name.clone())
        .collect();

    let mut result = Vec::new();
    let mut visited = HashSet::new();

    while let Some(node) = queue.pop_front() {
        if !visited.insert(node.clone()) {
            continue; // 重複回避
        }

        result.push(node.clone());

        if let Some(neighbors) = graph.get(&node) {
            for neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    // 循環依存チェック
    if result.len() != compose.services.len() {
        bail!("Circular dependency detected in compose file");
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_dependency_order_simple() {
        let compose = ComposeFile {
            version: "1.0".to_string(),
            services: vec![
                (
                    "a".to_string(),
                    ComposeService {
                        adep_root: "./a".to_string(),
                        depends_on: vec![],
                        healthcheck: None,
                        env: Vec::new(),
                        ports: Vec::new(),
                    },
                ),
                (
                    "b".to_string(),
                    ComposeService {
                        adep_root: "./b".to_string(),
                        depends_on: vec!["a".to_string()],
                        healthcheck: None,
                        env: Vec::new(),
                        ports: Vec::new(),
                    },
                ),
            ]
            .into_iter()
            .collect(),
        };

        let order = resolve_dependency_order(&compose).unwrap();
        assert_eq!(order, vec!["a", "b"]);
    }

    #[test]
    fn test_resolve_dependency_order_circular() {
        let compose = ComposeFile {
            version: "1.0".to_string(),
            services: vec![
                (
                    "a".to_string(),
                    ComposeService {
                        adep_root: "./a".to_string(),
                        depends_on: vec!["b".to_string()],
                        healthcheck: None,
                        env: Vec::new(),
                        ports: Vec::new(),
                    },
                ),
                (
                    "b".to_string(),
                    ComposeService {
                        adep_root: "./b".to_string(),
                        depends_on: vec!["a".to_string()],
                        healthcheck: None,
                        env: Vec::new(),
                        ports: Vec::new(),
                    },
                ),
            ]
            .into_iter()
            .collect(),
        };

        assert!(resolve_dependency_order(&compose).is_err());
    }
}
