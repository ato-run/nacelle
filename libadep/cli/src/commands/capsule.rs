use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command as SysCommand;
use std::io::Write;

use libadep_core::capsule_v1::CapsuleManifestV1;
use libadep_core::draft::{DraftAdvanced, DraftInput};
use libadep_core::resolver::Resolver;
use libadep_core::packager::{Packager, PackagerConfig, BuildMode, BuildPlan};

#[derive(Args)]
pub struct CapsuleArgs {
    #[command(subcommand)]
    pub command: CapsuleCommands,
}

#[derive(Subcommand)]
pub enum CapsuleCommands {
    /// Create a new capsule draft
    Create(CreateArgs),
    /// Show the build plan without executing (Dry run)
    Plan(PublishArgs),
    /// Build, push to registry, and update capsule.toml
    Publish(PublishArgs),
}

#[derive(Args)]
pub struct CreateArgs {
    /// Capsule name
    #[arg(long)]
    name: String,
    
    /// Target directory
    #[arg(default_value = ".")]
    path: PathBuf,
}

#[derive(Args)]
pub struct PublishArgs {
     /// Path to the source directory
    #[arg(short, long, default_value = ".")]
    source: PathBuf,

    /// Publish as a release version (updates 'latest' tag)
    #[arg(long)]
    release: bool,

    /// Enable debug mode (keep temporary files, verbose logs)
    #[arg(long)]
    debug: bool,

    /// Namespace for the registry image (overrides GUMBALL_NAMESPACE env)
    #[arg(long, env = "GUMBALL_NAMESPACE")]
    namespace: Option<String>,

    /// Registry host (e.g. localhost:5000). Overrides default registry.gumball.dev
    #[arg(long, env = "GUMBALL_REGISTRY_HOST")]
    registry_host: Option<String>,
    
    // Draft Overrides
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    start: Option<String>,
    #[arg(long)]
    port: Option<u16>,
}

pub fn run(args: CapsuleArgs) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run_async(args.command))
}

async fn run_async(command: CapsuleCommands) -> Result<()> {
    match command {
        CapsuleCommands::Create(args) => create(args).await,
        CapsuleCommands::Plan(args) => plan(args).await,
        CapsuleCommands::Publish(args) => publish(args).await,
    }
}

async fn create(args: CreateArgs) -> Result<()> {
    let path = args.path.join("capsule.toml");
    if path.exists() {
        return Err(anyhow!("capsule.toml already exists at {:?}", path));
    }

    let manifest = format!(r#"schema_version = "1.0"
name = "{}"
version = "0.1.0"
type = "app"

[metadata]
display_name = "{}"
"#, args.name, args.name); // Simple template

    fs::write(&path, manifest)?;
    println!("✅ Created capsule.toml at {:?}", path);
    Ok(())
}

async fn plan(args: PublishArgs) -> Result<()> {
    let (manifest, build_plan) = prepare_build(&args)?;
    
    println!("📋 Build Plan for {}", manifest.name);
    println!("   Mode: {}", if args.release { "Release" } else { "Snapshot" });
    println!("   Tag: {}", build_plan.primary_tag);
    println!("   Base: {}", build_plan.base_image);
    println!("   Context: {:?}", build_plan.context_path);
    println!("\nDockerfile:\n----------------\n{}\n----------------", build_plan.dockerfile_content);
    
    Ok(())
}

async fn publish(args: PublishArgs) -> Result<()> {
    let (mut manifest, build_plan) = prepare_build(&args)?;
    
    // 1. Prepare Workspace
    let gumball_dir = args.source.join(".gumball");
    let build_dir = gumball_dir.join("build");
    
    // Handle .gitignore
    ensure_gitignore(&args.source)?;

    if build_dir.exists() {
        fs::remove_dir_all(&build_dir)?;
    }
    fs::create_dir_all(&build_dir)?;

    // 2. Write Dockerfile
    let dockerfile_path = build_dir.join("Dockerfile");
    fs::write(&dockerfile_path, &build_plan.dockerfile_content)?;
    
    // 3. Execute Buildx
    println!("📦 Building capsule: {} -> {}", build_plan.context_path.display(), build_plan.primary_tag);
    
    let mut cmd = SysCommand::new("docker");
    cmd.args(["buildx", "build"]);
    if build_plan.push {
        cmd.arg("--push");
    }
    cmd.arg("--file").arg(&dockerfile_path);
    
    // Tags
    cmd.arg("-t").arg(&build_plan.primary_tag);
    for tag in &build_plan.additional_tags {
        cmd.arg("-t").arg(tag);
    }
    
    // Cache
    cmd.arg("--cache-from").arg(&build_plan.cache_from);
    cmd.arg("--cache-to").arg(&build_plan.cache_to);
    
    // Context
    cmd.arg(&build_plan.context_path);

    if args.debug {
        println!("Running: {:?}", cmd);
    }
    
    let status = cmd.status().context("Failed to execute docker buildx")?;
    
    if !status.success() {
        return Err(anyhow!("Docker build failed"));
    }

    // 4. Update Manifest & Save
    // Update execution config to point to the new image
    // Note: We update runtime to docker if not already set (it should be docker from resolver)
    manifest.execution.runtime = libadep_core::capsule_v1::RuntimeType::Docker;
    // For Release, use version tag. For Snapshot, use primary tag (dev-UTC).
    // Spec says: "Release uses version tag", "Snapshot uses dev-UTC". 
    // If we are saving to capsule.toml, standard practice is to reference the stable version or the dev ref?
    // Usually for dev loop, we might not want to dirty capsule.toml with dev tags?
    // BUT the spec says: "Update and save capsule.toml (Canonical)". 
    // And "Overwrite (Execution): [execution] section is forcibly overwritten".
    // So we update entrypoint/runtime.
    // However, if we write `registry...:dev-TIMESTAMP` into capsule.toml, it changes every time.
    // If the user commits this, it's messy.
    // Recommendation: For Snapshot, maybe NOT save the tag if it's transient?
    // Or save it but expect user to not commit?
    // Spec decision: "Update and save capsule.toml".
    // Let's assume we save the primary tag.
    manifest.execution.entrypoint = build_plan.primary_tag.clone();
    
    let toml = manifest.to_toml()?;
    let manifest_path = args.source.join("capsule.toml");
    fs::write(&manifest_path, toml)?;
    
    println!("✅ Published: {}", build_plan.primary_tag);
    println!("📝 Updated capsule.toml");

    // 5. Cleanup
    if !args.debug {
        let _ = fs::remove_dir_all(&build_dir);
    }

    Ok(())
}

fn prepare_build(args: &PublishArgs) -> Result<(CapsuleManifestV1, BuildPlan)> {
    let namespace = args.namespace.clone().ok_or_else(|| anyhow!("Namespace is required. Set GUMBALL_NAMESPACE or use --namespace"))?;
    
    // 1. Load Existing (Base)
    let manifest_path = args.source.join("capsule.toml");
    let existing_content = if manifest_path.exists() {
        Some(fs::read_to_string(&manifest_path)?)
    } else {
        None
    };

    prepare_build_internal(args, existing_content, &namespace)
}

fn prepare_build_internal(args: &PublishArgs, existing_toml: Option<String>, namespace: &str) -> Result<(CapsuleManifestV1, BuildPlan)> {
     // 1. Prepare Base Draft (Filesystem or Empty)
    let mut base_draft = if let Some(content) = existing_toml {
        let v1 = CapsuleManifestV1::from_toml(&content)?;
        draft_from_manifest(&v1)
    } else {
        DraftInput::default()
    };
    
    // 2. Merge CLI Args (Overlay)
    if let Some(n) = &args.name { base_draft.name = Some(n.clone()); }
    if let Some(s) = &args.start { 
        let adv = base_draft.advanced.get_or_insert(Default::default());
        adv.start = Some(s.clone()); 
    }
    if let Some(p) = args.port {
        let adv = base_draft.advanced.get_or_insert(Default::default());
        adv.port = Some(p);
    }
    
    // 3. Resolve
    let manifest = Resolver::resolve(&base_draft, &args.source)?;
    
    // 4. Plan
    let config = PackagerConfig {
        namespace: namespace.to_string(),
        registry_host: args.registry_host.clone(),
    };
    let mode = if args.release { BuildMode::Release } else { BuildMode::Snapshot };
    let packager = Packager::new(&config);
    
    let plan = packager.plan(&manifest, &args.source, mode)?;
    
    Ok((manifest, plan))
}

fn draft_from_manifest(v1: &CapsuleManifestV1) -> DraftInput {
    // Map V1 back to DraftInput for merging
    let adv = DraftAdvanced {
        type_: None, // Already fixed in V1, but can be re-inferred or kept?
        start: Some(v1.execution.entrypoint.clone()),
        port: v1.execution.port,
        env: Some(v1.execution.env.clone()),
        health_check: v1.execution.health_check.clone(),
        base_image: None,
    };
    
    DraftInput {
        name: Some(v1.name.clone()),
        version: Some(v1.version.clone()),
        display_name: v1.metadata.display_name.clone(),
        icon: v1.metadata.icon.clone(),
        description: v1.metadata.description.clone(),
        tags: Some(v1.metadata.tags.clone()),
        advanced: Some(adv),
    }
}

fn ensure_gitignore(path: &Path) -> Result<()> {
    let gitignore_path = path.join(".gitignore");
    if gitignore_path.exists() {
        let content = fs::read_to_string(&gitignore_path)?;
        if !content.contains(".gumball") {
            let mut file = fs::OpenOptions::new().append(true).open(&gitignore_path)?;
            writeln!(file, "\n# Gumball")?;
            writeln!(file, ".gumball/")?;
        }
    } else {
        let mut file = fs::File::create(&gitignore_path)?;
        writeln!(file, "# Gumball")?;
        writeln!(file, ".gumball/")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_merge_priority_cli_over_file() -> Result<()> {
        let dir = tempdir()?;
        // Create dummy package.json to satisfy resolver "App" detection if needed, 
        // OR we can rely on default if we don't specify type.
        // Resolver needs *something* to detect if type is not specified.
        use std::io::Write;
        let mut f = File::create(dir.path().join("package.json"))?;
        f.write_all(br#"{"scripts":{"start":"echo"}}"#)?;

        let existing_toml = r#"
            schema_version = "1.0"
            name = "file-name"
            version = "0.0.1"
            type = "app"
            
            [execution]
            runtime = "docker"
            entrypoint = "npm run file"
            port = 1111
        "#.to_string();

        let args = PublishArgs {
            source: dir.path().to_path_buf(),
            release: false, // Snapshot
            debug: false,
            namespace: Some("test-ns".into()),
            registry_host: None,
            name: Some("cli-name".into()), // Should override
            start: Some("npm run cli".into()), // Should override
            port: Some(2222), // Should override
        };

        let (manifest, plan) = prepare_build_internal(&args, Some(existing_toml), "test-ns")?;

        assert_eq!(manifest.name, "cli-name");
        assert_eq!(manifest.execution.entrypoint, "npm run cli");
        assert_eq!(manifest.execution.port, Some(2222));
        
        // Check Plan tags
        assert!(plan.primary_tag.contains("test-ns/cli-name:dev-"));
        Ok(())
    }

    #[tokio::test]
    async fn test_merge_priority_file_over_detect() -> Result<()> {
        let dir = tempdir()?;
        use std::io::Write;
        let mut f = File::create(dir.path().join("package.json"))?;
        f.write_all(br#"{"scripts":{"start":"echo"}}"#)?;

        let existing_toml = r#"
            schema_version = "1.0"
            name = "file-name"
            version = "0.0.1"
            type = "app"
            
            [execution]
            runtime = "docker"
            entrypoint = "npm run file"
            port = 4000
        "#.to_string();

        let args = PublishArgs {
            source: dir.path().to_path_buf(),
            release: false,
            debug: false,
            namespace: Some("test-ns".into()),
            registry_host: None,
            name: None,
            start: None,
            port: None,
        };

        let (manifest, _) = prepare_build_internal(&args, Some(existing_toml), "test-ns")?;

        assert_eq!(manifest.name, "file-name");
        assert_eq!(manifest.execution.entrypoint, "npm run file"); // File > Detect (npm start)
        assert_eq!(manifest.execution.port, Some(4000)); // File > Detect (3000)
        Ok(())
    }

    #[tokio::test]
    async fn test_snapshot_vs_release_tags() -> Result<()> {
        let dir = tempdir()?;
        File::create(dir.path().join("index.html"))?; // Static

        let args_snapshot = PublishArgs {
            source: dir.path().to_path_buf(),
            release: false,
            debug: false,
            namespace: Some("ns".into()),
            registry_host: None,
            name: Some("app".into()),
            start: None, port: None,
        };
        let (_, plan_snap) = prepare_build_internal(&args_snapshot, None, "ns")?;
        assert!(plan_snap.primary_tag.contains(":dev-"));
        assert!(plan_snap.additional_tags.is_empty());

        let args_release = PublishArgs {
            source: dir.path().to_path_buf(),
            release: true,
            debug: false,
            namespace: Some("ns".into()),
            registry_host: None,
            name: Some("app".into()),
            start: None, port: None,
        };
        let (_, plan_rel) = prepare_build_internal(&args_release, None, "ns")?;
        assert!(plan_rel.primary_tag.ends_with(":0.1.0"));
        assert_eq!(plan_rel.additional_tags, vec!["registry.gumball.dev/ns/app:latest"]);
        Ok(())
    }
}
