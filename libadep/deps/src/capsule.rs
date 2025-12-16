use crate::error::DepsdError;
use anyhow::{Context, Result};
use libadep_cas::IndexEntry;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tonic::Code;

const CAPSULE_SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, Deserialize)]
struct CapsuleJson {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    entries: Vec<IndexEntry>,
}

#[derive(Debug, Clone)]
pub struct CapsuleManifest {
    entries: Vec<IndexEntry>,
}

impl CapsuleManifest {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read capsule manifest {}", path.display()))?;
        let capsule: CapsuleJson = serde_json::from_str(&raw)
            .with_context(|| format!("invalid capsule manifest {}", path.display()))?;
        if capsule.schema_version != CAPSULE_SCHEMA_VERSION {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_INVALID_CAPSULE",
                format!(
                    "unsupported capsule schema version '{}' (expected {})",
                    capsule.schema_version, CAPSULE_SCHEMA_VERSION
                ),
            )
            .with_status(Code::InvalidArgument)
            .into_anyhow());
        }
        if capsule.entries.is_empty() {
            return Err(DepsdError::new(
                "E_ADEP_DEPS_INVALID_CAPSULE",
                format!("capsule manifest {} has no entries", path.display()),
            )
            .with_status(Code::InvalidArgument)
            .into_anyhow());
        }
        Ok(Self {
            entries: capsule.entries,
        })
    }

    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }
}
