use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::{ArgAction, Args};

use libadep_cas::{CompressedHash, Verifier};

use crate::manifest::{self, Manifest};
use crate::package;
use crate::signing;

#[derive(Args, Debug, Clone)]
pub struct VerifyArgs {
    /// Package root directory
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Skip signature verification (integrity only)
    #[arg(long, action = ArgAction::SetTrue)]
    pub skip_signature: bool,
}

pub fn run(args: &VerifyArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        cwd.join(&args.root)
    };

    let manifest_path = root.join("manifest.json");
    let manifest = Manifest::load(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;

    // Validate manifest metadata
    manifest::validate_manifest(&manifest).context("manifest validation failed")?;

    // Validate SBOM requirement
    if root.join("src").exists() && !root.join("sbom.json").exists() {
        bail!("src/ directory present but sbom.json is missing (required by ADEP spec)");
    }

    // Verify key rotation if present
    verify_key_rotation(&manifest)?;

    let recorded_manifest_sha = package::read_manifest_hash(&root)?;
    let actual_manifest_sha = package::hash_file_hex(&manifest_path)?;

    if recorded_manifest_sha != actual_manifest_sha {
        bail!(
            "manifest.json.sha256 mismatch (recorded={}, actual={})",
            recorded_manifest_sha,
            actual_manifest_sha
        );
    }

    let verifier = Verifier::new();
    let files_checked = verify_with_priority(&manifest, &root, &verifier)?;

    if !args.skip_signature {
        let sig_path = root.join("_sig").join("developer.sig");
        let sig = signing::read_signature_file(&sig_path)
            .with_context(|| format!("failed to read signature {}", sig_path.display()))?;

        let developer_key = manifest
            .developer_key
            .clone()
            .ok_or_else(|| anyhow::anyhow!("manifest missing developer_key"))?;
        signing::ensure_signature_matches_manifest(&sig, &developer_key)?;

        let package_digest = package::compute_package_digest(&root)?;
        signing::verify_signature_file(&sig, package_digest.as_bytes())?;

        if let Some(meta_digest) = sig.package_sha256() {
            if meta_digest != package_digest {
                bail!(
                    "signature metadata package_sha256 {} does not match computed {}",
                    meta_digest,
                    package_digest
                );
            }
        }

        println!(
            "Verification succeeded: manifest hash + {} files + developer signature",
            files_checked
        );
    } else {
        println!(
            "Verification succeeded: manifest hash + {} files (signature skipped)",
            files_checked
        );
    }

    Ok(())
}

fn verify_with_priority(manifest: &Manifest, root: &Path, verifier: &Verifier) -> Result<usize> {
    println!("Verifying runtime files first...");

    let mut runtime_count = 0;
    // Phase 1: runtime優先
    for entry in &manifest.files {
        if entry.role != "runtime" {
            continue;
        }
        let full_path = root.join(&entry.path);
        let verification = verify_file(entry, &full_path, verifier)
            .with_context(|| format!("failed to verify runtime file {}", entry.path))?;
        if verification != entry.size {
            bail!(
                "runtime file {} size mismatch (expected {}, actual {})",
                entry.path,
                entry.size,
                verification
            );
        }
        println!("  ✓ {}", entry.path);
        runtime_count += 1;
    }

    println!("Verifying remaining files...");

    let mut asset_count = 0;
    // Phase 2: 全ファイル
    for entry in &manifest.files {
        if entry.role == "runtime" {
            continue; // 既に検証済み
        }

        if entry.role != "runtime" {
            package::validate_role(&entry.role)
                .with_context(|| format!("invalid role for {}", entry.path))?;
        }

        let full_path = root.join(&entry.path);
        let verification = verify_file(entry, &full_path, verifier)
            .with_context(|| format!("failed to verify file {}", entry.path))?;
        if verification != entry.size {
            bail!(
                "file {} size mismatch (expected {}, actual {})",
                entry.path,
                entry.size,
                verification
            );
        }

        println!("  ✓ {}", entry.path);
        asset_count += 1;
    }

    Ok(runtime_count + asset_count)
}

fn verify_file(
    entry: &crate::manifest::FileEntry,
    full_path: &Path,
    verifier: &Verifier,
) -> Result<u64> {
    let compressed = entry.compressed.as_ref().map(|c| CompressedHash {
        alg: c.alg.clone(),
        sha256: c.sha256.clone(),
    });
    let result = verifier
        .verify(compressed, &entry.sha256, full_path)
        .map_err(|err| anyhow!("verification failed: {err}"))?;
    Ok(result.verified_bytes)
}

fn verify_key_rotation(manifest: &Manifest) -> Result<()> {
    if let Some(rotation) = &manifest.key_rotation {
        if let Some(prev_key) = &rotation.previous_key {
            // Validate previous_key format
            manifest::parse_developer_key(prev_key)
                .context("invalid previous_key format in key_rotation")?;

            // Display rotation info
            println!("\n⚠️  Key Rotation Detected:");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("  Previous: {}", prev_key);

            if let Some(current) = &manifest.developer_key {
                println!("  Current:  {}", current);
            }

            if let Some(reason) = &rotation.reason {
                println!("  Reason:   {}", reason);
            }

            if let Some(date) = &rotation.effective_date {
                println!("  Effective: {}", date);
            }

            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!(
                "ℹ️  Note: Full signature chain validation will be implemented in ADEP v1.3\n"
            );
        }
    }
    Ok(())
}
