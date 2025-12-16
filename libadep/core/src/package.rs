use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path};

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::manifest::{FileEntry, Manifest};

pub const PACKAGE_TOP_LEVEL: &[&str] = &[
    "manifest.json",
    "manifest.json.sha256",
    "dist",
    "_sig",
    "src",
    "sbom.json",
];

pub fn collect_dist_files(dist_dir: &Path, root: &Path) -> Result<Vec<FileEntry>> {
    if !dist_dir.exists() {
        bail!("dist directory {} does not exist", dist_dir.display());
    }

    let mut files = Vec::new();

    for entry in WalkDir::new(dist_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let full_path = entry.path();
        let rel_to_root = full_path
            .strip_prefix(root)
            .with_context(|| format!("{} is not within package root", full_path.display()))?;
        ensure_safe_relative_path(rel_to_root)?;

        let (sha256, size) = hash_file_with_size(full_path)?;
        let role = infer_role(full_path);
        files.push(FileEntry::new(
            to_unix_path(rel_to_root),
            sha256,
            size,
            role,
        ));
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

pub fn update_manifest_hash(root: &Path, manifest_path: &Path) -> Result<String> {
    let hash = hash_file_hex(manifest_path)?;
    let hash_path = root.join("manifest.json.sha256");
    fs::write(&hash_path, format!("{}\n", hash))
        .with_context(|| format!("failed to write {}", hash_path.display()))?;
    Ok(hash)
}

pub fn read_manifest_hash(root: &Path) -> Result<String> {
    let path = root.join("manifest.json.sha256");
    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(raw.trim().to_string())
}

#[allow(dead_code)]
pub fn verify_manifest_files(manifest: &Manifest, root: &Path) -> Result<usize> {
    let mut seen = HashSet::new();
    for entry in &manifest.files {
        if !seen.insert(entry.path.clone()) {
            bail!("duplicate file entry detected: {}", entry.path);
        }
        let rel_path = Path::new(&entry.path);
        ensure_safe_relative_path(rel_path)?;
        let full_path = root.join(rel_path);
        if !full_path.exists() {
            bail!("listed file missing on disk: {}", entry.path);
        }
        let (hash, size) = hash_file_with_size(&full_path)?;
        if hash != entry.sha256 {
            bail!(
                "hash mismatch for {} (manifest={}, actual={})",
                entry.path,
                entry.sha256,
                hash
            );
        }
        if size != entry.size {
            bail!(
                "size mismatch for {} (manifest={}, actual={})",
                entry.path,
                entry.size,
                size
            );
        }
        validate_role(&entry.role).with_context(|| format!("invalid role for {}", entry.path))?;
    }
    Ok(manifest.files.len())
}

pub fn compute_package_digest(root: &Path) -> Result<String> {
    let mut entries: Vec<(String, String)> = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let full_path = entry.path();
        let rel = full_path
            .strip_prefix(root)
            .with_context(|| format!("{} is not within package root", full_path.display()))?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        if !should_include_in_digest(rel)? {
            continue;
        }
        ensure_safe_relative_path(rel)?;
        let hash = hash_file_hex(full_path)?;
        entries.push((to_unix_path(rel), hash));
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (path, hash) in entries {
        hasher.update(path.as_bytes());
        hasher.update(&[0u8]);
        hasher.update(hash.as_bytes());
        hasher.update(&[0u8]);
    }

    Ok(hex::encode(hasher.finalize()))
}

pub fn hash_file_hex(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?,
    );
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn hash_file_with_size(path: &Path) -> Result<(String, u64)> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    let hash = hash_file_hex(path)?;
    Ok((hash, metadata.len()))
}

pub fn ensure_safe_relative_path(path: &Path) -> Result<()> {
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(anyhow!("invalid relative path {}", path.display()))
            }
            _ => {}
        }
    }
    Ok(())
}

fn should_include_in_digest(path: &Path) -> Result<bool> {
    match path.components().next() {
        Some(Component::Normal(name)) => {
            let Some(name_str) = name.to_str() else {
                return Ok(false);
            };
            if name_str == "_sig" {
                return Ok(false);
            }
            Ok(PACKAGE_TOP_LEVEL.iter().any(|allowed| name_str == *allowed))
        }
        _ => Ok(false),
    }
}

fn infer_role(path: &Path) -> String {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // *.worker.js パターン
    if file_name.ends_with(".worker.js")
        || file_name.ends_with(".worker.mjs")
        || file_name.ends_with(".worker.cjs")
    {
        return "runtime".to_string();
    }

    // .wasm のみ
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_ascii_lowercase())
        == Some("wasm".to_string())
    {
        return "runtime".to_string();
    }

    "asset".to_string()
}

fn to_unix_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Validate that role is either "runtime" or "asset"
pub fn validate_role(role: &str) -> Result<()> {
    match role {
        "runtime" | "asset" => Ok(()),
        _ => bail!("invalid role `{}`; expected runtime or asset", role),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn infer_role_runtime_exts() {
        assert_eq!(infer_role(Path::new("dist/app.worker.js")), "runtime");
        assert_eq!(infer_role(Path::new("dist/module.wasm")), "runtime");
        assert_eq!(infer_role(Path::new("dist/sw.worker.mjs")), "runtime");
    }

    #[test]
    fn infer_role_asset_default() {
        assert_eq!(infer_role(Path::new("dist/logo.png")), "asset");
        assert_eq!(infer_role(Path::new("dist/bundle.js")), "asset");
        assert_eq!(infer_role(Path::new("dist/app.js")), "asset");
    }

    #[test]
    fn parse_relative_path_validation() {
        assert!(ensure_safe_relative_path(Path::new("dist/app.js")).is_ok());
        assert!(ensure_safe_relative_path(Path::new("../secret")).is_err());
    }
}
