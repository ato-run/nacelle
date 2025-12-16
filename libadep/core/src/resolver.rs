use crate::capsule_v1::{
    CapsuleExecution, CapsuleManifestV1, CapsuleMetadataV1, CapsuleRequirements, CapsuleStorage,
    CapsuleType, RuntimeType,
};
use crate::draft::{DraftInput, DraftType};
use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Resolver;

impl Resolver {
    pub fn resolve(draft: &DraftInput, source_path: &Path) -> Result<CapsuleManifestV1> {
        let name = draft
            .name
            .clone()
            .or_else(|| {
                source_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| anyhow!("Name is required"))?;

        let version = draft.version.clone().unwrap_or_else(|| "0.1.0".to_string());

        // 1. Determine Type
        let detected_type = match draft.advanced.as_ref().and_then(|a| a.type_.clone()) {
            Some(t) => t,
            None => Self::detect_type(source_path)?,
        };

        let capsule_type = match detected_type {
            DraftType::Static | DraftType::App => CapsuleType::App,
            DraftType::Inference => CapsuleType::Inference,
            DraftType::Tool => CapsuleType::Tool,
        };

        // 2. Resolve Execution
        let (runtime, entrypoint, default_port, requirements) = match detected_type {
            DraftType::Static => (
                RuntimeType::Docker,
                "nginx".to_string(), // Placeholder, Packager will override with image
                Some(80),
                CapsuleRequirements::default(),
            ),
            DraftType::App => Self::resolve_app(source_path, draft)?,
            DraftType::Tool => Self::resolve_tool(source_path, draft)?,
            DraftType::Inference => return Err(anyhow!("Inference type auto-packaging is not supported in v0. Please define capsule.toml manually.")),
        };

        // Override with Draft Advanced
        let adv = draft.advanced.as_ref();
        let port = adv.and_then(|a| a.port).or(default_port);
        let start_cmd = adv.and_then(|a| a.start.clone()).unwrap_or(entrypoint);
        let env = adv.and_then(|a| a.env.clone()).unwrap_or_default();
        let health_check = adv.and_then(|a| a.health_check.clone());

        // Construct Manifest
        let manifest = CapsuleManifestV1 {
            schema_version: "1.0".to_string(),
            name,
            version,
            capsule_type,
            metadata: CapsuleMetadataV1 {
                display_name: draft.display_name.clone(),
                description: draft.description.clone(),
                icon: draft.icon.clone(),
                tags: draft.tags.clone().unwrap_or_default(),
                author: None, // Can be filled later
            },
            execution: CapsuleExecution {
                runtime,
                entrypoint: start_cmd, // Will be used as CMD in Dockerfile or entrypoint
                port,
                env,
                health_check,
                startup_timeout: 60, // Default constant
                signals: Default::default(),
            },
            requirements,
            capabilities: None,
            routing: Default::default(),
            network: None,
            model: None,
            storage: CapsuleStorage::default(),
        };

        Ok(manifest)
    }

    fn detect_type(path: &Path) -> Result<DraftType> {
        if path.join("package.json").exists() {
            return Ok(DraftType::App);
        }
        if path.join("requirements.txt").exists() {
            // Check for MCP signature
            if Self::has_file_content(path.join("requirements.txt"), "mcp")?
                || path.join("mcp_server.py").exists()
            {
                return Ok(DraftType::Tool);
            }
            return Ok(DraftType::App);
        }
        if path.join("index.html").exists() {
            return Ok(DraftType::Static);
        }

        Err(anyhow!(
            "Could not auto-detect project type. Please specify type in draft or capsule.toml."
        ))
    }

    fn resolve_app(
        path: &Path,
        draft: &DraftInput,
    ) -> Result<(RuntimeType, String, Option<u16>, CapsuleRequirements)> {
        // Node
        if path.join("package.json").exists() {
            // Check scripts.start
            let pkg_json = fs::read_to_string(path.join("package.json"))?;
            if !pkg_json.contains("\"start\"") {
                // Simple check, ideally parse JSON
                return Err(anyhow!(
                    "package.json missing `scripts.start`. Please define a start script."
                ));
            }
            return Ok((
                RuntimeType::Docker,
                "npm start".to_string(),
                Some(3000),
                CapsuleRequirements::default(),
            ));
        }

        // Python
        if path.join("requirements.txt").exists() {
            let entry = if let Some(s) = draft.advanced.as_ref().and_then(|a| a.start.clone()) {
                s
            } else if path.join("main.py").exists() {
                "python main.py".to_string()
            } else if path.join("app.py").exists() {
                "python app.py".to_string()
            } else {
                return Err(anyhow!("Entrypoint could not be detected. Please specify `main.py` or set start command."));
            };
            return Ok((
                RuntimeType::Docker,
                entry,
                Some(8000),
                CapsuleRequirements::default(),
            ));
        }

        Err(anyhow!("Unsupported App type"))
    }

    fn resolve_tool(
        path: &Path,
        draft: &DraftInput,
    ) -> Result<(RuntimeType, String, Option<u16>, CapsuleRequirements)> {
        // Assuming Python for Tools in v0 as per spec (requirements.txt + mcp)
        let entry = if let Some(s) = draft.advanced.as_ref().and_then(|a| a.start.clone()) {
            s
        } else if path.join("main.py").exists() {
            "python main.py".to_string()
        } else if path.join("app.py").exists() {
            "python app.py".to_string()
        } else {
            return Err(anyhow!("Entrypoint for Tool could not be detected."));
        };

        Ok((
            RuntimeType::Docker,
            entry,
            Some(8000),
            CapsuleRequirements::default(),
        ))
    }

    fn has_file_content(path: PathBuf, pattern: &str) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }
        let content = fs::read_to_string(path)?;
        Ok(content.contains(pattern))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_detect_node_app() -> Result<()> {
        let dir = tempdir()?;
        let pkg_json = r#"{ "scripts": { "start": "node server.js" } }"#;
        File::create(dir.path().join("package.json"))?.write_all(pkg_json.as_bytes())?;
        File::create(dir.path().join("server.js"))?;

        let type_ = Resolver::detect_type(dir.path())?;
        assert_eq!(type_, DraftType::App);

        let draft = DraftInput {
            name: Some("node-app".into()),
            ..Default::default()
        };
        let manifest = Resolver::resolve(&draft, dir.path())?;
        assert_eq!(manifest.capsule_type, CapsuleType::App);
        assert_eq!(manifest.execution.entrypoint, "npm start");
        Ok(())
    }

    #[test]
    fn test_detect_python_tool() -> Result<()> {
        let dir = tempdir()?;
        File::create(dir.path().join("requirements.txt"))?.write_all(b"mcp==0.1.0")?;
        File::create(dir.path().join("mcp_server.py"))?;
        File::create(dir.path().join("main.py"))?;

        let type_ = Resolver::detect_type(dir.path())?;
        assert_eq!(type_, DraftType::Tool);

        let draft = DraftInput {
            name: Some("py-tool".into()),
            ..Default::default()
        };
        let manifest = Resolver::resolve(&draft, dir.path())?;
        assert_eq!(manifest.capsule_type, CapsuleType::Tool);
        assert_eq!(manifest.execution.entrypoint, "python main.py");
        Ok(())
    }

    #[test]
    fn test_detect_python_app_fallback() -> Result<()> {
        let dir = tempdir()?;
        File::create(dir.path().join("requirements.txt"))?.write_all(b"flask")?;
        File::create(dir.path().join("app.py"))?;

        let type_ = Resolver::detect_type(dir.path())?;
        assert_eq!(type_, DraftType::App);

        let draft = DraftInput {
            name: Some("py-app".into()),
            ..Default::default()
        };
        let manifest = Resolver::resolve(&draft, dir.path())?;
        assert_eq!(manifest.capsule_type, CapsuleType::App);
        assert_eq!(manifest.execution.entrypoint, "python app.py");
        Ok(())
    }

    #[test]
    fn test_detect_static() -> Result<()> {
        let dir = tempdir()?;
        File::create(dir.path().join("index.html"))?;

        let type_ = Resolver::detect_type(dir.path())?;
        assert_eq!(type_, DraftType::Static);

        let draft = DraftInput {
            name: Some("static".into()),
            ..Default::default()
        };
        let manifest = Resolver::resolve(&draft, dir.path())?;
        assert_eq!(manifest.capsule_type, CapsuleType::App);
        assert_eq!(manifest.execution.runtime, RuntimeType::Docker);
        // Entrypoint placeholder for static, Packager will handle image
        assert_eq!(manifest.execution.entrypoint, "nginx");
        Ok(())
    }
}
