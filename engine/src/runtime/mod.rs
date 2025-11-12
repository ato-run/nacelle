use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use oci_spec::runtime::Spec;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::config::RuntimeSection;

const DEFAULT_HOOK_RETRY_ATTEMPTS: u32 = 1;

/// Runtime implementation to use for launching containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeKind {
    Youki,
    Runc,
}

impl RuntimeKind {
    fn from_str(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "youki" => Some(RuntimeKind::Youki),
            "runc" => Some(RuntimeKind::Runc),
            _ => None,
        }
    }

    fn binary_candidates(self) -> &'static [&'static str] {
        match self {
            RuntimeKind::Youki => &["youki"],
            RuntimeKind::Runc => &["runc"],
        }
    }
}

/// Concrete runtime configuration after resolving defaults and binary detection.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub kind: RuntimeKind,
    pub binary_path: PathBuf,
    pub bundle_root: PathBuf,
    pub state_root: PathBuf,
    pub log_dir: PathBuf,
    pub hook_retry_attempts: u32,
}

impl RuntimeConfig {
    pub fn from_section(section: Option<&RuntimeSection>) -> Result<Self, RuntimeError> {
        let preferred_kind = section
            .and_then(|s| s.preferred.as_deref())
            .and_then(RuntimeKind::from_str);

        let explicit_binary = section
            .and_then(|s| s.binary_path.as_ref())
            .map(PathBuf::from);

        let (kind, binary_path) = match (preferred_kind, explicit_binary) {
            (Some(kind), Some(path)) => {
                if path.exists() {
                    (kind, path)
                } else {
                    return Err(RuntimeError::BinaryNotFound {
                        tried: vec![path.to_string_lossy().into_owned()],
                    });
                }
            }
            (Some(kind), None) => {
                let path = find_binary(kind.binary_candidates())?;
                (kind, path)
            }
            (None, Some(path)) => {
                if !path.exists() {
                    return Err(RuntimeError::BinaryNotFound {
                        tried: vec![path.to_string_lossy().into_owned()],
                    });
                }
                let inferred_kind = infer_kind_from_path(&path).ok_or_else(|| {
                    RuntimeError::InvalidConfig(format!(
                        "Failed to infer runtime kind from binary path {:?}. Specify runtime.preferred",
                        path
                    ))
                })?;
                (inferred_kind, path)
            }
            (None, None) => {
                let order = [RuntimeKind::Youki, RuntimeKind::Runc];
                let mut attempts = Vec::new();
                let mut found = None;
                for kind in order {
                    match find_binary(kind.binary_candidates()) {
                        Ok(path) => {
                            found = Some((kind, path));
                            break;
                        }
                        Err(RuntimeError::BinaryNotFound { tried }) => {
                            attempts.extend(tried);
                        }
                        Err(err) => return Err(err),
                    }
                }
                match found {
                    Some(result) => result,
                    None => {
                        return Err(RuntimeError::BinaryNotFound { tried: attempts });
                    }
                }
            }
        };

        let base_dir = resolve_base_dir(section);
        let bundle_root = section
            .and_then(|s| s.bundle_root.as_ref())
            .map(PathBuf::from)
            .unwrap_or_else(|| base_dir.join("bundles"));
        let state_root = section
            .and_then(|s| s.state_root.as_ref())
            .map(PathBuf::from)
            .unwrap_or_else(|| base_dir.join("state"));
        let log_dir = section
            .and_then(|s| s.log_dir.as_ref())
            .map(PathBuf::from)
            .unwrap_or_else(|| base_dir.join("logs"));

        let hook_retry_attempts = section
            .and_then(|s| s.hook_retry_attempts)
            .unwrap_or(DEFAULT_HOOK_RETRY_ATTEMPTS);

        Ok(Self {
            kind,
            binary_path,
            bundle_root,
            state_root,
            log_dir,
            hook_retry_attempts,
        })
    }
}

fn resolve_base_dir(section: Option<&RuntimeSection>) -> PathBuf {
    if let Some(dir) = section.and_then(|s| s.bundle_root.as_ref()) {
        if let Some(parent) = Path::new(dir).parent() {
            return parent.to_path_buf();
        }
    }

    std::env::var("CAPSULED_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("capsuled"))
}

fn find_binary(candidates: &[&str]) -> Result<PathBuf, RuntimeError> {
    let mut tried = Vec::new();
    for candidate in candidates {
        match which::which(candidate) {
            Ok(path) => return Ok(path),
            Err(_) => tried.push(candidate.to_string()),
        }
    }
    Err(RuntimeError::BinaryNotFound { tried })
}

fn infer_kind_from_path(path: &Path) -> Option<RuntimeKind> {
    path.file_stem()
        .and_then(OsStr::to_str)
        .and_then(RuntimeKind::from_str)
}

/// High level request for launching a workload via OCI runtime.
#[derive(Debug)]
pub struct LaunchRequest<'a> {
    pub workload_id: &'a str,
    pub spec: &'a Spec,
    pub manifest_json: Option<&'a str>,
}

/// Result details returned after successful launch.
#[derive(Debug, Clone)]
pub struct LaunchResult {
    pub pid: u32,
    pub bundle_path: PathBuf,
    pub log_path: PathBuf,
}

/// Errors produced by the runtime launcher.
#[derive(thiserror::Error, Debug)]
pub enum RuntimeError {
    #[error("runtime binary not found. tried: {tried:?}")]
    BinaryNotFound { tried: Vec<String> },

    #[error("invalid runtime configuration: {0}")]
    InvalidConfig(String),

    #[error("I/O error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to serialize OCI spec: {0}")]
    SpecSerialization(serde_json::Error),

    #[error("runtime command failed: {operation} (exit={exit_code:?}): {stderr}")]
    CommandFailure {
        operation: String,
        exit_code: Option<i32>,
        stderr: String,
    },

    #[error("runtime command execution error ({operation}): {source}")]
    CommandExecution {
        operation: String,
        #[source]
        source: std::io::Error,
    },

    #[error("runtime state query failed: {source}")]
    StateQueryFailed {
        #[source]
        source: serde_json::Error,
    },
}

struct CommandOutput {
    stderr: String,
    exit_code: Option<i32>,
}

impl CommandOutput {
    fn into_error(self, operation: impl Into<String>) -> RuntimeError {
        RuntimeError::CommandFailure {
            operation: operation.into(),
            exit_code: self.exit_code,
            stderr: self.stderr,
        }
    }
}

/// Container runtime wrapper that launches workloads using runc/youki.
#[derive(Debug, Clone)]
pub struct ContainerRuntime {
    config: RuntimeConfig,
}

impl ContainerRuntime {
    pub fn new(config: RuntimeConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    /// Launch workload via runtime.
    pub async fn launch(&self, request: LaunchRequest<'_>) -> Result<LaunchResult, RuntimeError> {
        let bundle_path = self
            .prepare_bundle(request.workload_id, request.spec)
            .await?;
        let log_path = self.prepare_log_path(request.workload_id).await?;
        let pid_file = self
            .config
            .state_root
            .join(format!("{}.pid", request.workload_id));

        self.ensure_directory(&self.config.state_root).await?;

        self.write_manifest_snapshot(request.workload_id, request.manifest_json)
            .await?;

        let mut attempts = 0;
        loop {
            attempts += 1;
            match self
                .create_and_start(request.workload_id, &bundle_path, &pid_file, &log_path)
                .await
            {
                Ok(pid) => {
                    info!(
                        workload_id = request.workload_id,
                        pid, "runtime launched container"
                    );
                    return Ok(LaunchResult {
                        pid,
                        bundle_path,
                        log_path,
                    });
                }
                Err(err) => {
                    let (exit_code, stderr_text) = match &err {
                        RuntimeError::CommandFailure {
                            exit_code, stderr, ..
                        } => (*exit_code, Some(stderr.trim().to_string())),
                        _ => (None, None),
                    };

                    let should_retry = matches!(
                        &err,
                        RuntimeError::CommandFailure { stderr, .. }
                            if attempts <= self.config.hook_retry_attempts + 1
                                && hook_related_failure(stderr)
                    );

                    if should_retry {
                        warn!(
                            workload_id = request.workload_id,
                            attempt = attempts,
                            exit_code = ?exit_code,
                            stderr = stderr_text.as_deref().unwrap_or(""),
                            log_path = %log_path.display(),
                            "runtime reported hook failure; retrying"
                        );
                        self.cleanup_after_failure(request.workload_id).await.ok();
                        continue;
                    }

                    error!(
                        workload_id = request.workload_id,
                        attempts,
                        exit_code = ?exit_code,
                        stderr = stderr_text.as_deref().unwrap_or(""),
                        log_path = %log_path.display(),
                        ?err,
                        "runtime launch failed"
                    );

                    self.cleanup_after_failure(request.workload_id).await.ok();
                    return Err(err);
                }
            }
        }
    }

    async fn prepare_bundle(
        &self,
        workload_id: &str,
        spec: &Spec,
    ) -> Result<PathBuf, RuntimeError> {
        self.ensure_directory(&self.config.bundle_root).await?;
        let bundle_path = self.config.bundle_root.join(workload_id);
        self.ensure_directory(&bundle_path).await?;

        if let Some(root) = spec.root() {
            let path = root.path();
            if !path.exists() {
                return Err(RuntimeError::InvalidConfig(format!(
                    "root filesystem path {} does not exist",
                    path.display()
                )));
            }
        }

        let config_path = bundle_path.join("config.json");
        let spec_json = serde_json::to_vec_pretty(spec).map_err(RuntimeError::SpecSerialization)?;
        let mut file = fs::File::create(&config_path)
            .await
            .map_err(|source| RuntimeError::Io {
                path: config_path.clone(),
                source,
            })?;
        file.write_all(&spec_json)
            .await
            .map_err(|source| RuntimeError::Io {
                path: config_path.clone(),
                source,
            })?;
        file.flush().await.map_err(|source| RuntimeError::Io {
            path: config_path.clone(),
            source,
        })?;

        Ok(bundle_path)
    }

    async fn prepare_log_path(&self, workload_id: &str) -> Result<PathBuf, RuntimeError> {
        self.ensure_directory(&self.config.log_dir).await?;
        Ok(self.config.log_dir.join(format!("{}.log", workload_id)))
    }

    async fn write_manifest_snapshot(
        &self,
        workload_id: &str,
        manifest_json: Option<&str>,
    ) -> Result<(), RuntimeError> {
        if manifest_json.is_none() {
            return Ok(());
        }

        let snapshot_dir = self.config.bundle_root.join(workload_id);
        self.ensure_directory(&snapshot_dir).await?;
        let snapshot_path = snapshot_dir.join("manifest.json");
        let mut file =
            fs::File::create(&snapshot_path)
                .await
                .map_err(|source| RuntimeError::Io {
                    path: snapshot_path.clone(),
                    source,
                })?;
        file.write_all(manifest_json.unwrap().as_bytes())
            .await
            .map_err(|source| RuntimeError::Io {
                path: snapshot_path.clone(),
                source,
            })?;
        file.flush().await.map_err(|source| RuntimeError::Io {
            path: snapshot_path.clone(),
            source,
        })?;
        Ok(())
    }

    async fn create_and_start(
        &self,
        workload_id: &str,
        bundle_path: &Path,
        pid_file: &Path,
        log_path: &Path,
    ) -> Result<u32, RuntimeError> {
        match self
            .run_create(workload_id, bundle_path, pid_file, log_path)
            .await?
        {
            Ok(()) => {}
            Err(err) => return Err(err.into_error("create")),
        }

        match self.run_start(workload_id).await? {
            Ok(()) => {}
            Err(err) => return Err(err.into_error("start")),
        }

        let state = self.query_state(workload_id).await?;
        Ok(state.pid)
    }

    async fn run_create(
        &self,
        workload_id: &str,
        bundle_path: &Path,
        pid_file: &Path,
        log_path: &Path,
    ) -> Result<Result<(), CommandOutput>, RuntimeError> {
        let mut cmd = Command::new(&self.config.binary_path);
        cmd.arg("create")
            .arg("--root")
            .arg(&self.config.state_root)
            .arg("--bundle")
            .arg(bundle_path)
            .arg("--pid-file")
            .arg(pid_file)
            .arg("--log")
            .arg(log_path)
            .arg("--log-format")
            .arg("json")
            .arg(workload_id);

        let result = self.spawn_command(cmd, "create").await?;
        if let Err(ref output) = result {
            if !output.stderr.is_empty() {
                debug!(workload_id, "runtime create stderr: {}", output.stderr);
            }
        }
        Ok(result)
    }

    async fn run_start(
        &self,
        workload_id: &str,
    ) -> Result<Result<(), CommandOutput>, RuntimeError> {
        let mut cmd = Command::new(&self.config.binary_path);
        cmd.arg("start")
            .arg("--root")
            .arg(&self.config.state_root)
            .arg(workload_id);

        self.spawn_command(cmd, "start").await
    }

    async fn spawn_command(
        &self,
        mut cmd: Command,
        operation: &str,
    ) -> Result<Result<(), CommandOutput>, RuntimeError> {
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());
        debug!(operation = %operation, "executing runtime command: {:?}", cmd);

        match cmd.output().await {
            Ok(output) => {
                if output.status.success() {
                    Ok(Ok(()))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    Ok(Err(CommandOutput {
                        stderr,
                        exit_code: output.status.code(),
                    }))
                }
            }
            Err(source) => Err(RuntimeError::CommandExecution {
                operation: operation.to_string(),
                source,
            }),
        }
    }

    async fn query_state(&self, workload_id: &str) -> Result<RuntimeState, RuntimeError> {
        let mut cmd = Command::new(&self.config.binary_path);
        cmd.arg("state")
            .arg("--root")
            .arg(&self.config.state_root)
            .arg(workload_id)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = match cmd.output().await {
            Ok(output) => output,
            Err(source) => {
                return Err(RuntimeError::CommandExecution {
                    operation: "state".to_string(),
                    source,
                })
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            warn!(
                workload_id,
                "runtime state command failed: {}",
                stderr.trim()
            );
            return Err(RuntimeError::CommandFailure {
                operation: "state".to_string(),
                exit_code: output.status.code(),
                stderr,
            });
        }

        serde_json::from_slice(&output.stdout)
            .map_err(|source| RuntimeError::StateQueryFailed { source })
    }

    async fn cleanup_after_failure(&self, workload_id: &str) -> Result<(), RuntimeError> {
        let mut cmd = Command::new(&self.config.binary_path);
        cmd.arg("delete")
            .arg("--force")
            .arg("--root")
            .arg(&self.config.state_root)
            .arg(workload_id)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        match cmd.output().await {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!(
                        workload_id,
                        "runtime delete failed during cleanup: {}",
                        stderr.trim()
                    );
                }
                Ok(())
            }
            Err(source) => {
                warn!(
                    workload_id,
                    error = %source,
                    "failed to execute runtime delete for cleanup"
                );
                Ok(())
            }
        }
    }

    async fn ensure_directory(&self, path: &Path) -> Result<(), RuntimeError> {
        if path.exists() {
            return Ok(());
        }
        fs::create_dir_all(path)
            .await
            .map_err(|source| RuntimeError::Io {
                path: path.to_path_buf(),
                source,
            })
    }
}

#[derive(Debug, serde::Deserialize)]
struct RuntimeState {
    pid: u32,
}

fn hook_related_failure(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("hook") || lower.contains("nvidia")
}

impl From<RuntimeError> for CommandOutput {
    fn from(err: RuntimeError) -> Self {
        match err {
            RuntimeError::CommandFailure {
                stderr, exit_code, ..
            } => CommandOutput { stderr, exit_code },
            _ => CommandOutput {
                stderr: err.to_string(),
                exit_code: None,
            },
        }
    }
}
