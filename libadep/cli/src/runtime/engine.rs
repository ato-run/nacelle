use anyhow::Result;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedEngine {
    Podman,
    Docker,
}

impl ResolvedEngine {
    pub fn command_name(&self) -> &'static str {
        match self {
            ResolvedEngine::Podman => "podman",
            ResolvedEngine::Docker => "docker",
        }
    }
}

pub fn has_podman() -> Result<bool> {
    Ok(Command::new("podman")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false))
}

pub fn has_docker() -> Result<bool> {
    Ok(Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false))
}

pub fn resolve_engine(engine_spec: &str) -> Result<ResolvedEngine> {
    match engine_spec {
        "podman" => {
            if has_podman()? {
                Ok(ResolvedEngine::Podman)
            } else {
                Err(crate::error::podman_required().into())
            }
        }
        "auto" => {
            if has_podman()? {
                println!("ℹ️  Using podman (auto-detected)");
                Ok(ResolvedEngine::Podman)
            } else if has_docker()? {
                println!("ℹ️  Using docker (podman not found)");
                Ok(ResolvedEngine::Docker)
            } else {
                Err(crate::error::no_container_engine().into())
            }
        }
        "docker" => Err(crate::error::docker_direct_not_yet().into()),
        _ => Err(crate::error::unsupported_engine(engine_spec).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_podman() {
        // podman検出
        let result = has_podman();
        assert!(result.is_ok());
    }

    #[test]
    fn test_has_docker() {
        // docker検出
        let result = has_docker();
        assert!(result.is_ok());
    }
}
