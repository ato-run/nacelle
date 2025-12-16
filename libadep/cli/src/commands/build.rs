use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;

use crate::manifest::Manifest;
use crate::package;

#[derive(Args, Debug, Clone)]
pub struct BuildArgs {
    /// Package root directory
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Location of built artifacts (relative to root unless absolute)
    #[arg(long, default_value = "dist")]
    pub dist: PathBuf,
    /// Manifest path (relative to root unless absolute)
    #[arg(long, default_value = "manifest.json")]
    pub manifest: PathBuf,
}

pub fn run(args: &BuildArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        cwd.join(&args.root)
    };

    let manifest_path = if args.manifest.is_absolute() {
        args.manifest.clone()
    } else {
        root.join(&args.manifest)
    };

    let dist_path = if args.dist.is_absolute() {
        args.dist.clone()
    } else {
        root.join(&args.dist)
    };

    if !dist_path.exists() {
        bail!("dist directory {} does not exist", dist_path.display());
    }

    let mut manifest = Manifest::load(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;

    // Validate manifest metadata
    crate::manifest::validate_manifest(&manifest).context("manifest validation failed")?;

    let files = package::collect_dist_files(&dist_path, &root)?;
    manifest.files = files;

    manifest
        .save(&manifest_path)
        .with_context(|| format!("failed to update {}", manifest_path.display()))?;

    package::update_manifest_hash(&root, &manifest_path)?;

    println!(
        "Updated manifest files entry ({} artifacts)",
        manifest.files.len()
    );

    Ok(())
}
