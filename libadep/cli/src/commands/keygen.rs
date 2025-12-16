use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{ArgAction, Args};

use crate::manifest::Manifest;
use crate::package;
use crate::signing::StoredKey;

#[derive(Args, Debug, Clone)]
pub struct KeygenArgs {
    /// Package root (used to locate manifest.json if present)
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Output path for the generated key file
    #[arg(long, default_value = "keys/developer.json")]
    pub out: PathBuf,
    /// Explicit manifest path (defaults to <root>/manifest.json)
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// Overwrite existing key file
    #[arg(long)]
    pub force: bool,
    /// Skip manifest developer_key update even if manifest.json exists
    #[arg(long, action = ArgAction::SetTrue)]
    pub skip_manifest: bool,
}

pub fn run(args: &KeygenArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        cwd.join(&args.root)
    };

    let key_path = if args.out.is_absolute() {
        args.out.clone()
    } else {
        root.join(&args.out)
    };

    if key_path.exists() && !args.force {
        bail!(
            "key file {} already exists; pass --force to overwrite",
            key_path.display()
        );
    }

    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create key directory {}", parent.display()))?;
    }

    let stored = StoredKey::generate();
    stored
        .write(&key_path)
        .with_context(|| format!("failed to write key file {}", key_path.display()))?;

    println!("Generated developer keypair -> {}", key_path.display());
    println!("Public key: {}", stored.developer_key_fingerprint());

    let manifest_path = args
        .manifest
        .clone()
        .unwrap_or_else(|| root.join("manifest.json"));

    if args.skip_manifest || !manifest_path.exists() {
        return Ok(());
    }

    let mut manifest = Manifest::load(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    manifest.developer_key = Some(stored.developer_key_fingerprint());
    manifest
        .save(&manifest_path)
        .with_context(|| format!("failed to update {}", manifest_path.display()))?;

    package::update_manifest_hash(&root, &manifest_path)?;

    println!(
        "Updated manifest developer_key in {}",
        manifest_path.display()
    );
    Ok(())
}
