use crate::artifacts::{detect_entry_kind, entry_filename, materialize_artifact, CapsuleEntryKind};
use crate::capsule::CapsuleManifest;
use crate::error::DepsdError;
use crate::proto::command_log::Stream as CommandStream;
use crate::proto::{CommandLog, InstallPythonRequest, InstallPythonResponse};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tonic::Code;

#[derive(Clone, Default)]
pub struct PythonHandler;

impl PythonHandler {
    pub fn new() -> Self {
        Self
    }

    pub fn install(&self, request: &InstallPythonRequest) -> Result<InstallPythonResponse> {
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
        let requirements_lock = PathBuf::from(&request.requirements_lock);
        if !requirements_lock.exists() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_INPUT_NOT_FOUND",
                format!(
                    "requirements file {} does not exist",
                    requirements_lock.display()
                ),
            )
            .with_status(Code::InvalidArgument)
            .into_anyhow());
        }
        let wheels_root = PathBuf::from(&request.target_dir);
        fs::create_dir_all(&wheels_root)
            .with_context(|| format!("failed to create {}", wheels_root.display()))?;

        let capsule = CapsuleManifest::load(capsule_path)?;
        let mut wheel_entries = Vec::new();
        for entry in capsule.entries() {
            if detect_entry_kind(entry) == CapsuleEntryKind::PythonWheel {
                wheel_entries.push(entry);
            }
        }
        if wheel_entries.is_empty() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_NO_ARTIFACTS",
                format!(
                    "capsule {} does not include python-wheel entries",
                    capsule_path.display()
                ),
            )
            .with_status(Code::FailedPrecondition)
            .into_anyhow());
        }

        let mut installed_paths = Vec::new();
        for entry in wheel_entries {
            let file_name = entry_filename(entry)?;
            let outcome = materialize_artifact(&cas_root, entry, &wheels_root, file_name)?;
            installed_paths.push(outcome.path);
        }

        let pip_bin = if request.pip_binary.trim().is_empty() {
            "pip".to_string()
        } else {
            request.pip_binary.clone()
        };
        let mut command = vec![pip_bin.clone(), "install".into()];
        command.extend([
            "--require-hashes".into(),
            "--no-deps".into(),
            "--no-index".into(),
            "--no-input".into(),
            "--disable-pip-version-check".into(),
            "--find-links".into(),
            wheels_root.display().to_string(),
            "-r".into(),
            requirements_lock.display().to_string(),
        ]);
        command.extend(request.pip_args.iter().cloned());

        if request.dry_run {
            return Ok(InstallPythonResponse {
                command,
                exit_code: 0,
                logs: Vec::new(),
                requirements_lock: requirements_lock.display().to_string(),
                installed_wheels: installed_paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect(),
            });
        }

        let mut cmd = Command::new(&pip_bin);
        let working_dir = if request.project_dir.trim().is_empty() {
            requirements_lock.parent().map(|p| p.to_path_buf())
        } else {
            Some(PathBuf::from(&request.project_dir))
        };
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }
        cmd.args(&command[1..]);
        cmd.env("PIP_NO_INDEX", "1");
        cmd.env("PIP_DISABLE_PIP_VERSION_CHECK", "1");
        cmd.env("PIP_RETRIES", "0");
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
                format!("pip install failed with status {}", exit_code),
            )
            .with_status(Code::Aborted)
            .into_anyhow());
        }

        Ok(InstallPythonResponse {
            command,
            exit_code,
            logs,
            requirements_lock: requirements_lock.display().to_string(),
            installed_wheels: installed_paths
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
