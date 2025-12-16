use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use chrono::{Utc, DateTime};
use crate::capsule_v1::{CapsuleManifestV1, RuntimeType};

pub const BASE_IMAGE_NODE: &str = "node:20-alpine";
pub const BASE_IMAGE_PYTHON: &str = "python:3.11-slim-bookworm";
pub const BASE_IMAGE_NGINX: &str = "nginx:1.25-alpine";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildMode {
    Release,
    Snapshot,
}

#[derive(Debug, Clone)]
pub struct PackagerConfig {
    pub namespace: String,
    pub registry_host: Option<String>,
}

impl Default for PackagerConfig {
    fn default() -> Self {
        Self {
            namespace: "default".to_string(),
            registry_host: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildPlan {
    pub context_path: PathBuf,
    pub dockerfile_content: String,
    pub primary_tag: String,
    pub additional_tags: Vec<String>,
    pub cache_from: String,
    pub cache_to: String,
    pub push: bool,
    pub base_image: String,
}

pub struct Packager<'a> {
    config: &'a PackagerConfig,
}

impl<'a> Packager<'a> {
    pub fn new(config: &'a PackagerConfig) -> Self {
        Self { config }
    }

    pub fn plan(&self, manifest: &CapsuleManifestV1, source_path: &Path, mode: BuildMode) -> Result<BuildPlan> {
        let registry = self.config.registry_host.as_deref().unwrap_or("registry.gumball.dev");
        let repo = format!("{}/{}/{}", registry, self.config.namespace, manifest.name);
        
        // 1. Calculate Tags
        let (primary_tag, additional_tags) = match mode {
            BuildMode::Release => (
                format!("{}:{}", repo, manifest.version),
                vec![format!("{}:latest", repo)]
            ),
            BuildMode::Snapshot => {
                let now: DateTime<Utc> = Utc::now();
                let params = now.format("%Y%m%d-%H%M%S").to_string();
                (format!("{}:dev-{}", repo, params), vec![])
            }
        };

        // 2. Cache Config
        let cache_tag = format!("{}:cache", repo);

        // 3. Generate Dockerfile
        let (dockerfile_content, base_image) = self.generate_dockerfile(manifest)?;

        Ok(BuildPlan {
            context_path: source_path.to_path_buf(),
            dockerfile_content,
            primary_tag,
            additional_tags,
            cache_from: format!("type=registry,ref={}", cache_tag),
            cache_to: format!("type=registry,ref={},mode=max", cache_tag),
            push: true,
            base_image: base_image.to_string(),
        })
    }

    fn generate_dockerfile(&self, manifest: &CapsuleManifestV1) -> Result<(String, &str)> {
        match manifest.execution.runtime {
             RuntimeType::Docker => {
                 // Try to guess based on requirements/logic if we generated this manifest with specific base logic
                 // But strictly, we should look at what the manifest says effectively.
                 // However, "runtime=docker" in v1 usually means "use this image". 
                 // Here we are *building* the image. 
                 // So we need to infer from source structure again OR rely on what Resolver populated?
                 // Resolver didn't populate "base_image" in execution (it populated 'entrypoint' and 'runtime').
                 // We need to look at files again or encode it in manifest somehow? 
                 // v0 spec said Resolver logic mapped to specific bases. 
                 // Let's re-detect strictly for Dockerfile generation, or assume standard patterns.
                 
                 // Simpler: Check requirements (Node vs Python vs Static)
                 // NOTE: Since Resolver runs before Packager, we can trust the source files exist.
                 // Ideally Packager shouldn't re-detect, but we don't carry "detected_type" in manifest.
                 // Wait, Resolver returns CapsuleManifestV1. 
                 // In v0, we can infer from `entrypoint` or file existence to pick the template.
                 
                 // Node
                 if manifest.execution.entrypoint.contains("npm") {
                     return Ok((format!(
                         "FROM {}\nWORKDIR /app\nCOPY package*.json ./\nRUN npm ci\nCOPY . .\nEXPOSE {}\nCMD (npm start)", 
                         BASE_IMAGE_NODE, manifest.execution.port.unwrap_or(3000)
                     ).replace("(", "[").replace(")", "]").replace(" ", "\", \""), BASE_IMAGE_NODE));
                 }
                 
                 // Static (Nginx) - heuristic: entrypoint "nginx"
                 if manifest.execution.entrypoint == "nginx" {
                     return Ok((format!(
                         "FROM {}\nCOPY . /usr/share/nginx/html\nEXPOSE 80\nCMD [\"nginx\", \"-g\", \"daemon off;\"]", 
                         BASE_IMAGE_NGINX
                     ), BASE_IMAGE_NGINX));
                 }

                 // Python
                 if manifest.execution.entrypoint.contains("python") {
                     // Parse entrypoint string "python main.py" -> ["python", "main.py"]
                     let parts: Vec<&str> = manifest.execution.entrypoint.split_whitespace().collect();
                     let cmd_json = serde_json::to_string(&parts).unwrap_or_else(|_| "[]".to_string());
                     
                     return Ok((format!(
                         "FROM {}\nWORKDIR /app\nCOPY requirements.txt ./\nRUN pip install --no-cache-dir -r requirements.txt\nCOPY . .\nEXPOSE {}\nCMD {}",
                         BASE_IMAGE_PYTHON, manifest.execution.port.unwrap_or(8000), cmd_json
                     ), BASE_IMAGE_PYTHON));
                 }

                 Err(anyhow!("Unsupported runtime/entrypoint for auto-generation: {}", manifest.execution.entrypoint))
             },
             _ => Err(anyhow!("Packager only supports Docker runtime packaging")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capsule_v1::{CapsuleManifestV1, CapsuleType, RuntimeType, CapsuleExecution, CapsuleMetadataV1};

    fn mock_manifest(entrypoint: &str) -> CapsuleManifestV1 {
        CapsuleManifestV1 {
            schema_version: "1.0".into(),
            name: "test-app".into(),
            version: "0.1.0".into(),
            capsule_type: CapsuleType::App,
            metadata: CapsuleMetadataV1::default(),
            execution: CapsuleExecution {
                runtime: RuntimeType::Docker,
                entrypoint: entrypoint.into(),
                port: Some(8080),
                ..base_execution()
            },
            requirements: Default::default(),
            capabilities: None,
            routing: Default::default(),
            model: None,
            storage: Default::default(),
        }
    }

    fn base_execution() -> CapsuleExecution {
        // ... (can use Default if we implemented it, or construct manually)
        unsafe { std::mem::zeroed() } // Quick dirty way for test? No, let's correspond to struct.
        // Actually simpler to just construct minimal struct in test
        unimplemented!()
    }
    
    // Helper to bypass full struct construction in every test
    fn create_manifest(entrypoint: &str) -> CapsuleManifestV1 {
        // Use a minimal valid JSON and parse it? Or construct?
        // Let's construct.
        CapsuleManifestV1 {
            schema_version: "1.0".into(),
            name: "test-app".into(),
            version: "0.1.0".into(),
            capsule_type: CapsuleType::App,
            metadata: Default::default(),
            execution: CapsuleExecution {
                runtime: RuntimeType::Docker,
                entrypoint: entrypoint.into(),
                port: Some(8080),
                health_check: None,
                startup_timeout: 60,
                env: Default::default(),
                signals: Default::default(),
            },
            requirements: Default::default(),
            capabilities: None,
            routing: Default::default(),
            model: None,
            storage: Default::default(),
        }
    }

    #[test]
    fn test_plan_release() {
        let config = PackagerConfig { namespace: "ekoh".into(), ..Default::default() };
        let packager = Packager::new(&config);
        let manifest = create_manifest("npm start");
        
        let plan = packager.plan(&manifest, Path::new("."), BuildMode::Release).unwrap();
        
        assert_eq!(plan.primary_tag, "registry.gumball.dev/ekoh/test-app:0.1.0");
        assert_eq!(plan.additional_tags, vec!["registry.gumball.dev/ekoh/test-app:latest"]);
        assert!(plan.base_image.contains("node"));
    }

    #[test]
    fn test_plan_snapshot() {
        let config = PackagerConfig { namespace: "ekoh".into(), ..Default::default() };
        let packager = Packager::new(&config);
        let manifest = create_manifest("python main.py");
        
        let plan = packager.plan(&manifest, Path::new("."), BuildMode::Snapshot).unwrap();
        
        assert!(plan.primary_tag.contains("dev-"));
        assert!(plan.additional_tags.is_empty());
        assert!(plan.base_image.contains("python"));
    }
    
    #[test]
    fn test_nginx_generation() {
        let config = PackagerConfig::default();
        let packager = Packager::new(&config);
        let manifest = create_manifest("nginx");
        
        let plan = packager.plan(&manifest, Path::new("."), BuildMode::Release).unwrap();
        assert!(plan.base_image.contains("nginx"));
        assert!(plan.dockerfile_content.contains("COPY . /usr/share/nginx/html"));
    }
}
