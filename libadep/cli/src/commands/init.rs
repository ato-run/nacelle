use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use uuid::Uuid;

use crate::manifest::{Manifest, PublishInfo};
use crate::package;

#[derive(Args, Debug, Clone)]
pub struct InitArgs {
    /// ADEP package root directory
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Optional application display name
    #[arg(long)]
    pub app_name: Option<String>,
    /// Initial release channel (stable|beta|canary)
    #[arg(long, default_value = "stable")]
    pub channel: String,
    /// Initial semantic version number
    #[arg(long, default_value = "0.1.0")]
    pub version: String,
    /// Create dist dir at a custom path relative to root
    #[arg(long)]
    pub dist: Option<PathBuf>,
    /// Overwrite existing manifest if present
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: &InitArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        cwd.join(&args.root)
    };
    fs::create_dir_all(&root)
        .with_context(|| format!("failed to create root directory {}", root.display()))?;

    let manifest_path = root.join("manifest.json");
    if manifest_path.exists() && !args.force {
        bail!("manifest.json already exists; pass --force to overwrite");
    }

    let mut manifest = Manifest::template(Some(args.version.clone()), Some(args.channel.clone()));
    manifest.id = Uuid::new_v4();
    manifest.family_id = Uuid::new_v4();

    // Set publish_info with defaults
    let mut info = manifest.publish_info.unwrap_or_else(PublishInfo::default);

    if let Some(name) = &args.app_name {
        info.name = Some(name.clone());
    } else if info.name.is_none() {
        info.name = Some("My ADEP App".to_string());
    }

    // Set default icon if not specified
    if info.icon.is_none() {
        info.icon = Some("dist/icon.png".to_string());
    }

    manifest.publish_info = Some(info);

    manifest.files.clear();
    manifest
        .save(&manifest_path)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let dist_dir = args
        .dist
        .as_ref()
        .map(|p| {
            if p.is_absolute() {
                p.clone()
            } else {
                root.join(p)
            }
        })
        .unwrap_or_else(|| root.join("dist"));
    fs::create_dir_all(&dist_dir)
        .with_context(|| format!("failed to create dist directory {}", dist_dir.display()))?;

    let sig_dir = root.join("_sig");
    fs::create_dir_all(&sig_dir)
        .with_context(|| format!("failed to create signature directory {}", sig_dir.display()))?;

    package::update_manifest_hash(&root, &manifest_path)?;

    println!("Initialized ADEP package at {}", root.display());
    Ok(())
}
