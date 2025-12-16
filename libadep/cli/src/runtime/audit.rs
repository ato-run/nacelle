use anyhow::{Context, Result};
use chrono::Utc;
use dirs::home_dir;
use serde::Serialize;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
pub struct AuditGuard {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl AuditGuard {
    #[allow(dead_code)]
    pub fn proxy_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// コンテナから到達可能なプロキシURL
    pub fn proxy_url_for_container(&self) -> String {
        let port = self.addr.port();

        // Linux: host.containers.internal を使用（--add-host で追加済み）
        #[cfg(target_os = "linux")]
        {
            format!("http://host.containers.internal:{}", port)
        }

        // macOS: host.docker.internal（Docker Desktop）または host-gateway
        #[cfg(target_os = "macos")]
        {
            format!("http://host.docker.internal:{}", port)
        }

        // Windows: host.docker.internal
        #[cfg(target_os = "windows")]
        {
            format!("http://host.docker.internal:{}", port)
        }
    }

    #[allow(dead_code)]
    pub fn stop(mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AuditGuard {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Serialize)]
struct AuditRecord<'a> {
    ts: String,
    app: &'a str,
    runtime: &'a str,
    host: &'a str,
    path: &'a str,
    method: &'a str,
    status: u16,
    bytes: u64,
}

pub fn spawn(app: String, runtime: String) -> Result<AuditGuard> {
    let listener = TcpListener::bind("127.0.0.1:0").context("failed to bind audit proxy")?;
    listener
        .set_nonblocking(true)
        .context("failed to configure audit proxy")?;
    let addr = listener.local_addr()?;
    let log_path = audit_log_path()?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let handle = thread::spawn(move || {
        if let Err(err) = run_proxy(listener, log_path, app, runtime, shutdown_clone) {
            eprintln!("ADEP audit proxy exited: {err:?}");
        }
    });
    Ok(AuditGuard {
        addr,
        shutdown,
        handle: Some(handle),
    })
}

fn audit_log_path() -> Result<PathBuf> {
    let home = home_dir().ok_or_else(|| crate::error::home_dir_unavailable())?;
    let dir = home.join(".adep");
    create_dir_all(&dir).map_err(|err| {
        crate::error::audit_proxy_failed(format!("failed to create {}: {err}", dir.display()))
    })?;
    Ok(dir.join("audit.log"))
}

fn run_proxy(
    listener: TcpListener,
    log_path: PathBuf,
    app: String,
    runtime: String,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let log_path = log_path.clone();
                let app = app.clone();
                let runtime = runtime.clone();
                thread::spawn(move || {
                    if let Err(err) = handle_client(stream, &log_path, &app, &runtime) {
                        eprintln!("audit proxy client error: {err:?}");
                    }
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(15));
            }
            Err(err) => {
                eprintln!("audit proxy accept error: {err:?}");
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

fn handle_client(mut client: TcpStream, log_path: &Path, app: &str, runtime: &str) -> Result<()> {
    client.set_read_timeout(Some(Duration::from_secs(20))).ok();
    let (head, body) = read_request_head(&mut client)?;
    if head.is_empty() {
        return Ok(());
    }
    let head_str = String::from_utf8_lossy(&head);
    let mut parts = head_str
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("HTTP/1.1");
    if method.eq_ignore_ascii_case("CONNECT") {
        return handle_connect(method, target, client, log_path, app, runtime);
    }
    let (host, port) = resolve_host_port(target, &head_str)?;
    let path = extract_path(target);
    let mut server = TcpStream::connect(format!("{host}:{port}")).map_err(|err| {
        crate::error::audit_proxy_failed(format!("connect {host}:{port} failed: {err}"))
    })?;
    server.set_read_timeout(Some(Duration::from_secs(20))).ok();
    let filtered_head = rebuild_request_head(method, &path, version, &head_str);
    server.write_all(filtered_head.as_bytes())?;
    if !body.is_empty() {
        server.write_all(&body)?;
    }
    let mut total = 0u64;
    let mut status = 0u16;
    let mut first_chunk = Vec::new();
    loop {
        let mut buf = [0u8; 8192];
        let read = server.read(&mut buf)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if status == 0 {
            first_chunk.extend_from_slice(&buf[..read]);
            if let Some((code, consumed)) = try_parse_status(&first_chunk) {
                status = code;
                client.write_all(&first_chunk[..consumed])?;
                if first_chunk.len() > consumed {
                    client.write_all(&first_chunk[consumed..])?;
                }
                continue;
            }
        }
        client.write_all(&buf[..read])?;
    }
    if status == 0 {
        status = 502;
    }
    append_log(
        log_path,
        AuditRecord {
            ts: Utc::now().to_rfc3339(),
            app,
            runtime,
            host: &host,
            path: &path,
            method,
            status,
            bytes: total,
        },
    )?;
    Ok(())
}

fn handle_connect(
    method: &str,
    target: &str,
    mut client: TcpStream,
    log_path: &Path,
    app: &str,
    runtime: &str,
) -> Result<()> {
    let mut parts = target.split(':');
    let host = parts.next().unwrap_or("");
    let port = parts
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(443);
    let mut server = TcpStream::connect(format!("{host}:{port}")).map_err(|err| {
        crate::error::audit_proxy_failed(format!("connect {host}:{port} failed: {err}"))
    })?;
    client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")?;
    let down = Arc::new(AtomicU64::new(0));
    let up = Arc::new(AtomicU64::new(0));
    let mut client_clone = client.try_clone()?;
    let mut server_clone = server.try_clone()?;
    let down_clone = down.clone();
    let up_clone = up.clone();
    let t1 = thread::spawn(move || copy_count(&mut server_clone, &mut client_clone, down_clone));
    let t2 = thread::spawn(move || copy_count(&mut client, &mut server, up_clone));
    let _ = t1.join();
    let _ = t2.join();
    append_log(
        log_path,
        AuditRecord {
            ts: Utc::now().to_rfc3339(),
            app,
            runtime,
            host,
            path: "",
            method,
            status: 200,
            bytes: down.load(Ordering::Relaxed) + up.load(Ordering::Relaxed),
        },
    )?;
    Ok(())
}

fn copy_count(reader: &mut TcpStream, writer: &mut TcpStream, counter: Arc<AtomicU64>) {
    let mut buf = [0u8; 8192];
    while let Ok(n) = reader.read(&mut buf) {
        if n == 0 {
            break;
        }
        if writer.write_all(&buf[..n]).is_err() {
            break;
        }
        counter.fetch_add(n as u64, Ordering::Relaxed);
    }
}

fn append_log(path: &Path, record: AuditRecord<'_>) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| {
            crate::error::audit_proxy_failed(format!("failed to open {}: {err}", path.display()))
        })?;
    serde_json::to_writer(&mut file, &record).map_err(|err| {
        crate::error::audit_proxy_failed(format!("failed to serialize audit entry: {err}"))
    })?;
    file.write_all(b"\n")?;
    Ok(())
}

fn read_request_head(stream: &mut TcpStream) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1];

    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => {
                head.push(buf[0]);
                // HTTPヘッダ終端検出: \r\n\r\n
                if head.len() >= 4 && head.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                // WouldBlock 時は短時間スリープして忙待ちを回避
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok((head, Vec::new()))
}

fn resolve_host_port(target: &str, head: &str) -> Result<(String, u16)> {
    if let Some(stripped) = target.strip_prefix("http://") {
        let parts: Vec<&str> = stripped.splitn(2, '/').collect();
        let host_port = parts[0];
        if let Some((host, port)) = host_port.split_once(':') {
            return Ok((host.to_string(), port.parse().unwrap_or(80)));
        }
        return Ok((host_port.to_string(), 80));
    }
    for line in head.lines() {
        if line.to_lowercase().starts_with("host:") {
            let host_value = line[5..].trim();
            if let Some((host, port)) = host_value.split_once(':') {
                return Ok((host.to_string(), port.parse().unwrap_or(80)));
            }
            return Ok((host_value.to_string(), 80));
        }
    }
    Ok(("unknown".to_string(), 80))
}

fn extract_path(target: &str) -> String {
    if let Some(stripped) = target.strip_prefix("http://") {
        let parts: Vec<&str> = stripped.splitn(2, '/').collect();
        if parts.len() > 1 {
            return format!("/{}", parts[1]);
        }
    }
    if target.starts_with('/') {
        return target.to_string();
    }
    "/".to_string()
}

fn rebuild_request_head(method: &str, path: &str, version: &str, original_head: &str) -> String {
    let mut result = format!("{} {} {}\r\n", method, path, version);
    for line in original_head.lines().skip(1) {
        if line.to_lowercase().starts_with("proxy-") {
            continue;
        }
        result.push_str(line);
        result.push_str("\r\n");
    }
    result.push_str("\r\n");
    result
}

fn try_parse_status(data: &[u8]) -> Option<(u16, usize)> {
    let s = String::from_utf8_lossy(data);
    let mut lines = s.lines();
    let first_line = lines.next()?;
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let status_code = parts[1].parse::<u16>().ok()?;
    let header_end = s.find("\r\n\r\n")?;
    Some((status_code, header_end + 4))
}
