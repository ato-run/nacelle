use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::copy;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;
use zip::write::FileOptions;
use zip::CompressionMethod;

use crate::manifest::Manifest;
use crate::package::{self, PACKAGE_TOP_LEVEL};

#[derive(Args, Debug, Clone)]
pub struct PackArgs {
    /// Package root directory
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Output archive path (defaults to <root>/app.adep)
    #[arg(long, default_value = "app.adep")]
    pub output: PathBuf,
    /// Root directory name inside the archive
    #[arg(long, default_value = "app.adep")]
    pub package_dir: String,
    /// Overwrite output if it already exists
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: &PackArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        cwd.join(&args.root)
    };

    let output_path = if args.output.is_absolute() {
        args.output.clone()
    } else {
        root.join(&args.output)
    };

    if output_path.exists() && !args.force {
        bail!(
            "output file {} already exists; pass --force to overwrite",
            output_path.display()
        );
    }

    let manifest_path = root.join("manifest.json");
    let manifest = Manifest::load(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;

    package::update_manifest_hash(&root, &manifest_path)?;
    ensure_signature_exists(&root)?;

    // Validate SBOM requirement
    if root.join("src").exists() && !root.join("sbom.json").exists() {
        bail!(
            "src/ directory present but sbom.json is missing.\n\
             ADEP spec requires SBOM (SPDX 2.3 or CycloneDX 1.4 JSON) when source code is included.\n\
             See: https://spdx.dev/specifications/ or https://cyclonedx.org/specification/overview/"
        );
    }

    let mut files = Vec::new();
    let mut directories: BTreeSet<PathBuf> = BTreeSet::new();

    for entry in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
    {
        let full_path = entry.path();
        if full_path == output_path {
            continue;
        }
        let rel = full_path
            .strip_prefix(&root)
            .expect("walkdir entry should contain prefix");
        if rel.as_os_str().is_empty() {
            continue;
        }
        if !is_included(rel) {
            continue;
        }
        if entry.file_type().is_dir() {
            directories.insert(rel.to_path_buf());
            continue;
        }
        if entry.file_type().is_file() {
            files.push(rel.to_path_buf());
            if let Some(parent) = rel.parent() {
                if !parent.as_os_str().is_empty() {
                    directories.insert(parent.to_path_buf());
                }
            }
        }
    }

    files.sort();

    if files.is_empty() {
        bail!("no packageable files found under {}", root.display());
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }

    let archive = File::create(&output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;
    let mut writer = zip::ZipWriter::new(archive);

    let base = args.package_dir.trim_end_matches('/');
    let dir_options = FileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).unwrap())
        .unix_permissions(0o755);
    let file_options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).unwrap())
        .unix_permissions(0o644);

    writer.add_directory(format!("{}/", base), dir_options)?;

    for dir in &directories {
        let dir_path = format!("{}/{}/", base, to_unix_path(dir));
        writer.add_directory(dir_path, dir_options)?;
    }

    for rel in &files {
        let zip_path = format!("{}/{}", base, to_unix_path(rel));
        writer.start_file(zip_path, file_options)?;
        let mut src = File::open(root.join(rel))
            .with_context(|| format!("failed to read {}", rel.display()))?;
        copy(&mut src, &mut writer)?;
    }

    writer.finish()?;

    let package_digest = package::compute_package_digest(&root)?;
    println!(
        "Packed {} files -> {} (digest {})",
        files.len(),
        output_path.display(),
        package_digest
    );
    println!(
        "Manifest {} version {}",
        manifest.family_id, manifest.version.number
    );

    Ok(())
}

fn ensure_signature_exists(root: &Path) -> Result<()> {
    let sig_path = root.join("_sig").join("developer.sig");
    if !sig_path.exists() {
        bail!(
            "developer signature missing at {}; run `adep sign` first",
            sig_path.display()
        );
    }
    Ok(())
}

fn is_included(path: &Path) -> bool {
    match path.components().next() {
        Some(Component::Normal(name)) => {
            if let Some(name_str) = name.to_str() {
                PACKAGE_TOP_LEVEL.iter().any(|allowed| name_str == *allowed)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn to_unix_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
