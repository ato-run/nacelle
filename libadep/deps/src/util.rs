use crate::error::DepsdError;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Component, Path};
use tonic::Code;

pub fn ensure_safe_relative_path(path: &Path) -> Result<()> {
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(DepsdError::new(
                    "E_ADEP_DEPS_INVALID_PATH",
                    format!("invalid relative path {}", path.display()),
                )
                .with_status(Code::InvalidArgument)
                .into_anyhow());
            }
            _ => {}
        }
    }
    Ok(())
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
