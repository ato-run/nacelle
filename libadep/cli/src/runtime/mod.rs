mod audit;
mod container;
mod engine;
pub mod registry;

use crate::error;
use crate::manifest::{Manifest, RuntimeProfile};
use anyhow::{bail, ensure, Result};
use std::path::Path;

pub use container::execute_container;
pub use engine::resolve_engine;
pub use registry::AdepRegistry;

pub fn execute_manifest(manifest: &Manifest, root_path: &Path) -> Result<()> {
    let profile = manifest
        .runtime_profile()
        .ok_or_else(|| error::no_runtime_specified())?;
    validate(&profile)?;
    let engine = resolve_engine(&profile.runtime.engine)?;
    execute_container(
        engine,
        manifest,
        profile.runtime,
        profile.platform,
        root_path,
    )
}

fn validate(profile: &RuntimeProfile<'_>) -> Result<()> {
    match profile.runtime.runtime_type.as_str() {
        "container" => {}
        t => bail!(error::unsupported_type(t)),
    }
    match profile.runtime.engine.as_str() {
        "podman" | "auto" | "docker" => {}
        e => bail!(error::unsupported_engine(e)),
    }
    match profile.platform.language.as_str() {
        "python" => {}
        lang => bail!(error::unsupported_language(lang)),
    }
    let valid_versions = ["3.9", "3.10", "3.11", "3.12"];
    ensure!(
        valid_versions.contains(&profile.platform.version.as_str()),
        error::unsupported_version(&profile.platform.language, &profile.platform.version)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{PlatformSpec, RuntimeProfile, RuntimeSpec};
    use std::path::PathBuf;

    #[test]
    fn test_runtime_validate_valid() {
        let runtime = RuntimeSpec {
            runtime_type: "container".into(),
            engine: "podman".into(),
            image: None,
        };
        let platform = PlatformSpec {
            language: "python".into(),
            version: "3.11".into(),
            entry: PathBuf::from("main.py"),
            dependencies: None,
            wheels: None,
        };
        let profile = RuntimeProfile {
            runtime: &runtime,
            platform: &platform,
        };
        assert!(validate(&profile).is_ok());
    }

    #[test]
    fn test_runtime_validate_invalid_version() {
        let runtime = RuntimeSpec {
            runtime_type: "container".into(),
            engine: "podman".into(),
            image: None,
        };
        let platform = PlatformSpec {
            language: "python".into(),
            version: "2.7".into(), // 不正
            entry: PathBuf::from("main.py"),
            dependencies: None,
            wheels: None,
        };
        let profile = RuntimeProfile {
            runtime: &runtime,
            platform: &platform,
        };
        assert!(validate(&profile).is_err());
    }

    #[test]
    fn test_runtime_validate_invalid_type() {
        let runtime = RuntimeSpec {
            runtime_type: "wasm".into(), // 不正
            engine: "podman".into(),
            image: None,
        };
        let platform = PlatformSpec {
            language: "python".into(),
            version: "3.11".into(),
            entry: PathBuf::from("main.py"),
            dependencies: None,
            wheels: None,
        };
        let profile = RuntimeProfile {
            runtime: &runtime,
            platform: &platform,
        };
        assert!(validate(&profile).is_err());
    }
}
