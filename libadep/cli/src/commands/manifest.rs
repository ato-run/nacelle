use std::path::{Path, PathBuf};

use crate::manifest::{self, validate_manifest, DefaultOptions, Manifest};
use libadep::deps::defaults::StaticDefaults;
use crate::package;
use anyhow::{Context, Result};
use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct ManifestArgs {
    #[command(subcommand)]
    pub command: ManifestCommand,
}

#[derive(Subcommand, Debug)]
pub enum ManifestCommand {
    /// Migrate manifest.json to a newer schema version
    Migrate(MigrateArgs),
}

#[derive(Args, Debug, Clone)]
pub struct MigrateArgs {
    /// ADEP package root directory
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Target schemaVersion (default: 1.2)
    #[arg(long, default_value = "1.2")]
    pub target: String,
    /// Preview changes without writing manifest.json
    #[arg(long)]
    pub dry_run: bool,
    /// Optional output path (defaults to manifest.json in root)
    #[arg(long)]
    pub output: Option<PathBuf>,
}

pub fn run(args: ManifestArgs) -> Result<()> {
    match args.command {
        ManifestCommand::Migrate(args) => run_migrate(&args),
    }
}

fn resolve_root(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir().context("failed to resolve current directory")?;
        Ok(cwd.join(path))
    }
}

fn run_migrate(args: &MigrateArgs) -> Result<()> {
    let root = resolve_root(&args.root)?;
    let manifest_path = root.join("manifest.json");
    let mut manifest = Manifest::load(&manifest_path)
        .with_context(|| format!("failed to load {}", manifest_path.display()))?;

    let dependency_defaults = StaticDefaults::new();
    let defaults = manifest::apply_defaults(
        &mut manifest,
        DefaultOptions::default().with_dependency_defaults(&dependency_defaults),
    );

    manifest.schema_version = args.target.clone();

    validate_manifest(&manifest).context("manifest validation failed after migration")?;

    if args.dry_run {
        let json = serde_json::to_string_pretty(&manifest)?;
        println!("{json}");
        for warning in &defaults.warnings {
            eprintln!("⚠️  {}: {}", warning.code, warning.message);
        }
        return Ok(());
    }

    let output_path = if let Some(out) = &args.output {
        if out.is_absolute() {
            out.clone()
        } else {
            root.join(out)
        }
    } else {
        manifest_path.clone()
    };

    manifest
        .save(&output_path)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    // Update manifest.json.sha256 when writing to the canonical manifest path
    if output_path == manifest_path {
        package::update_manifest_hash(&root, &manifest_path)?;
    }

    println!(
        "✓ Migrated manifest schemaVersion to {} at {}",
        manifest.schema_version,
        output_path.display()
    );

    for warning in &defaults.warnings {
        eprintln!("⚠️  {}: {}", warning.code, warning.message);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_manifest_migrate_updates_schema() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let manifest_path = root.join("manifest.json");
        let mut manifest = Manifest::template(Some("0.1.0".into()), Some("stable".into()));
        manifest.schema_version = "1.0".into();
        manifest.save(&manifest_path).unwrap();
        package::update_manifest_hash(root, &manifest_path).unwrap();

        let args = MigrateArgs {
            root: root.to_path_buf(),
            target: "1.2".into(),
            dry_run: false,
            output: None,
        };
        run_migrate(&args).unwrap();

        let migrated: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        assert_eq!(migrated["schemaVersion"], "1.2");
        assert_eq!(migrated["pack"]["profile"], "dist+cas");

        let hash_path = root.join("manifest.json.sha256");
        assert!(
            hash_path.exists(),
            "manifest.json.sha256 should be updated after migrate"
        );
        let hash_contents = fs::read_to_string(hash_path).unwrap();
        let new_hash = package::hash_file_hex(&manifest_path).unwrap();
        assert!(
            hash_contents.trim() == new_hash,
            "manifest.json.sha256 should match migrated manifest"
        );
    }

    #[test]
    fn test_manifest_migrate_dry_run_keeps_original_file() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let manifest_path = root.join("manifest.json");
        let mut manifest = Manifest::template(Some("0.1.0".into()), Some("stable".into()));
        manifest.schema_version = "1.0".into();
        manifest.save(&manifest_path).unwrap();

        let args = MigrateArgs {
            root: root.to_path_buf(),
            target: "1.2".into(),
            dry_run: true,
            output: None,
        };
        run_migrate(&args).unwrap();

        let reread: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        assert_eq!(reread["schemaVersion"], "1.0");
    }
}
