use crate::artifacts::{detect_entry_kind, entry_filename, materialize_artifact, CapsuleEntryKind};
use crate::capsule::CapsuleManifest;
use crate::error::DepsdError;
use crate::proto::command_log::Stream as CommandStream;
use crate::proto::{CommandLog, InstallPnpmRequest, InstallPnpmResponse};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tonic::Code;

#[derive(Clone, Default)]
pub struct PnpmHandler;

impl PnpmHandler {
    pub fn new() -> Self {
        Self
    }

    pub fn install(&self, request: &InstallPnpmRequest) -> Result<InstallPnpmResponse> {
        let capsule_path = Path::new(&request.capsule_path);
        if !capsule_path.exists() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_CAPSULE_NOT_FOUND",
                format!("capsule manifest {} does not exist", capsule_path.display()),
            )
            .with_status(Code::InvalidArgument)
            .into_anyhow());
        }
        let cas_root = PathBuf::from(&request.cas_root);
        if !cas_root.exists() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_CAS_NOT_FOUND",
                format!("CAS root {} does not exist", cas_root.display()),
            )
            .with_status(Code::InvalidArgument)
            .into_anyhow());
        }
        let lockfile_path = PathBuf::from(&request.lockfile);
        if !lockfile_path.exists() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_INPUT_NOT_FOUND",
                format!("pnpm lockfile {} does not exist", lockfile_path.display()),
            )
            .with_status(Code::InvalidArgument)
            .into_anyhow());
        }
        let project_dir = if request.project_dir.trim().is_empty() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_INVALID_REQUEST",
                "project_dir must be provided for pnpm install requests",
            )
            .with_status(Code::InvalidArgument)
            .into_anyhow());
        } else {
            PathBuf::from(&request.project_dir)
        };
        if !project_dir.exists() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_INPUT_NOT_FOUND",
                format!("project directory {} does not exist", project_dir.display()),
            )
            .with_status(Code::InvalidArgument)
            .into_anyhow());
        }
        let store_dir = PathBuf::from(&request.store_dir);
        fs::create_dir_all(&store_dir)
            .with_context(|| format!("failed to create {}", store_dir.display()))?;

        let capsule = CapsuleManifest::load(capsule_path)?;
        let mut node_entries = Vec::new();
        for entry in capsule.entries() {
            if detect_entry_kind(entry) == CapsuleEntryKind::PnpmTarball {
                node_entries.push(entry);
            }
        }
        if node_entries.is_empty() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_NO_ARTIFACTS",
                format!(
                    "capsule {} does not include pnpm tarball entries",
                    capsule_path.display()
                ),
            )
            .with_status(Code::FailedPrecondition)
            .into_anyhow());
        }

        let mut stored_paths = Vec::new();
        for entry in node_entries {
            let file_name = entry_filename(entry)?;
            let outcome = materialize_artifact(&cas_root, entry, &store_dir, file_name)?;
            stored_paths.push(outcome.path);
        }

        let pnpm_bin = if request.pnpm_binary.trim().is_empty() {
            "pnpm".to_string()
        } else {
            request.pnpm_binary.clone()
        };
        let mut command = vec![pnpm_bin.clone(), "install".into()];
        command.extend([
            "--offline".into(),
            "--frozen-lockfile".into(),
            "--store-dir".into(),
            store_dir.display().to_string(),
        ]);
        command.extend(request.pnpm_args.iter().cloned());

        if request.dry_run {
            return Ok(InstallPnpmResponse {
                command,
                exit_code: 0,
                logs: Vec::new(),
                installed_packages: stored_paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect(),
            });
        }

        let mut cmd = Command::new(&pnpm_bin);
        cmd.current_dir(&project_dir);
        cmd.args(&command[1..]);
        cmd.env("PNPM_FETCH_RETRIES", "0");
        cmd.env("PNPM_NETWORK_CONCURRENCY", "1");
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let output = cmd
            .output()
            .with_context(|| format!("failed to execute '{}'", command.join(" ")))?;
        let exit_code = output.status.code().unwrap_or(-1);
        let mut logs = Vec::new();
        logs.extend(collect_logs(CommandStream::Stdout, &output.stdout));
        logs.extend(collect_logs(CommandStream::Stderr, &output.stderr));
        if !output.status.success() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_COMMAND_FAILED",
                format!("pnpm install failed with status {}", exit_code),
            )
            .with_status(Code::Aborted)
            .into_anyhow());
        }

        Ok(InstallPnpmResponse {
            command,
            exit_code,
            logs,
            installed_packages: stored_paths
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        })
    }
}

fn collect_logs(stream: CommandStream, bytes: &[u8]) -> Vec<CommandLog> {
    let mut logs = Vec::new();
    if bytes.is_empty() {
        return logs;
    }
    let text = String::from_utf8_lossy(bytes);
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        logs.push(CommandLog {
            stream: stream as i32,
            line: line.to_string(),
        });
    }
    logs
}
