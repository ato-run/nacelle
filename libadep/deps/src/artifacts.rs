use crate::error::DepsdError;
use crate::util::{ensure_safe_relative_path, hash_file_hex};
use anyhow::{anyhow, Context, Result};
use libadep_cas::index::IndexMetadata;
use libadep_cas::{safety, IndexEntry};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use tonic::Code;
use zstd::stream::Decoder as ZstdDecoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsuleEntryKind {
    PythonWheel,
    PnpmTarball,
    Other,
}

#[derive(Debug, Clone)]
pub struct MaterializeOutcome {
    pub path: PathBuf,
}

pub fn detect_entry_kind(entry: &IndexEntry) -> CapsuleEntryKind {
    if let Some(IndexMetadata {
        kind: Some(kind), ..
    }) = &entry.metadata
    {
        match kind.as_str() {
            "python-wheel" => return CapsuleEntryKind::PythonWheel,
            "pnpm-tarball" => return CapsuleEntryKind::PnpmTarball,
            _ => {}
        }
    }
    for coord in &entry.coords {
        if coord.starts_with("pkg:pypi/") {
            return CapsuleEntryKind::PythonWheel;
        }
        if coord.starts_with("pkg:npm/") {
            return CapsuleEntryKind::PnpmTarball;
        }
    }
    CapsuleEntryKind::Other
}

pub fn materialize_artifact(
    cas_root: &Path,
    entry: &IndexEntry,
    dest_dir: &Path,
    file_name: &str,
) -> Result<MaterializeOutcome> {
    ensure_safe_relative_path(Path::new(file_name))?;
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("failed to create {}", dest_dir.display()))?;
    let dest_path = dest_dir.join(file_name);

    if dest_path.exists() {
        let existing_sha = hash_file_hex(&dest_path)?;
        if existing_sha == entry.raw_sha256 {
            return Ok(MaterializeOutcome { path: dest_path });
        }
        fs::remove_file(&dest_path).with_context(|| {
            format!("failed to remove outdated artifact {}", dest_path.display())
        })?;
    }

    ensure_safe_relative_path(Path::new(&entry.path))?;
    let source_path = cas_root.join("blobs").join(&entry.path);
    if !source_path.exists() {
        return Err(DepsdError::new(
            "E_ADEP_DEPS_BLOB_NOT_FOUND",
            format!(
                "CAS blob '{}' missing at {}; ensure deps capsule is synced",
                entry.path,
                source_path.display()
            ),
        )
        .with_status(Code::FailedPrecondition)
        .into_anyhow());
    }
    let file = fs::File::open(&source_path)
        .with_context(|| format!("failed to open blob {}", source_path.display()))?;
    let mut reader: Box<dyn Read> = match entry.compressed.as_ref() {
        Some(compressed) => match compressed.alg.as_str() {
            "zstd" => {
                let decoder = ZstdDecoder::new(BufReader::new(file)).map_err(|err| {
                    anyhow!("failed to decompress {}: {err}", source_path.display())
                })?;
                if let Some(compressed_size) = compressed.size {
                    let raw_size = entry.size.unwrap_or_default();
                    safety::enforce_compression_ratio(raw_size, compressed_size)
                        .map_err(|err| DepsdError::from(err).into_anyhow())?;
                }
                Box::new(decoder)
            }
            other => {
                return Err(DepsdError::new(
                    "E_ADEP_DEPS_UNSUPPORTED_COMPRESSION",
                    format!(
                        "unsupported compression algorithm '{}' for entry '{}'",
                        other, entry.path
                    ),
                )
                .with_status(Code::InvalidArgument)
                .into_anyhow());
            }
        },
        None => Box::new(BufReader::new(file)),
    };

    let mut temp = NamedTempFile::new_in(dest_dir)
        .with_context(|| format!("failed to create temp file in {}", dest_dir.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read blob {}", entry.path))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        temp.write_all(&buffer[..read])
            .with_context(|| format!("failed to write temp artifact {}", file_name))?;
    }
    temp.flush()
        .with_context(|| format!("failed to flush temp artifact {}", file_name))?;
    let digest = hex::encode(hasher.finalize());
    if digest != entry.raw_sha256 {
        return Err(DepsdError::new(
            "E_ADEP_DEPS_HASH_MISMATCH",
            format!(
                "raw hash mismatch for '{}' (expected {}, computed {})",
                entry.path, entry.raw_sha256, digest
            ),
        )
        .with_status(Code::FailedPrecondition)
        .into_anyhow());
    }
    temp.persist(&dest_path).map_err(|err| {
        anyhow!(
            "failed to persist artifact {}: {}",
            dest_path.display(),
            err.error
        )
    })?;
    Ok(MaterializeOutcome { path: dest_path })
}

pub fn entry_filename(entry: &IndexEntry) -> Result<&str> {
    entry
        .metadata
        .as_ref()
        .and_then(|m| m.filename.as_deref())
        .ok_or_else(|| {
            DepsdError::new(
                "E_ADEP_DEPS_METADATA_MISSING",
                format!("capsule entry '{}' missing metadata.filename", entry.path),
            )
            .with_status(Code::FailedPrecondition)
            .into_anyhow()
        })
}
