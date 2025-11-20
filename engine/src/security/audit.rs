use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// AuditRecord represents a single signed audit log entry
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditRecord {
    pub timestamp: String,
    pub node_id: String,
    pub gpu_uuid: Option<String>,
    pub action: String,
    pub status: String,
    pub details: Option<String>,
    pub signature: Option<String>,
}

impl AuditRecord {
    /// Create the string representation to be signed
    /// Format: timestamp|node_id|gpu_uuid|action|status|details
    pub fn signable_string(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            self.timestamp,
            self.node_id,
            self.gpu_uuid.as_deref().unwrap_or(""),
            self.action,
            self.status,
            self.details.as_deref().unwrap_or("")
        )
    }
}

/// AuditLogger handles signing and writing audit logs
pub struct AuditLogger {
    log_path: PathBuf,
    signing_key: SigningKey,
    node_id: String,
    file_lock: Mutex<()>,
}

impl AuditLogger {
    /// Create a new AuditLogger.
    /// If key_path exists, load the key. Otherwise, generate a new one and save it.
    pub fn new(log_path: PathBuf, key_path: PathBuf, node_id: String) -> Result<Self> {
        // Ensure log directory exists
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent).context("Failed to create audit log directory")?;
        }

        // Ensure key directory exists
        if let Some(parent) = key_path.parent() {
            fs::create_dir_all(parent).context("Failed to create key directory")?;
        }

        // Load or generate key
        let signing_key = if key_path.exists() {
            info!("Loading audit key from {:?}", key_path);
            let key_bytes = fs::read(&key_path).context("Failed to read audit key")?;
            let key_array: [u8; 32] = key_bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid key length"))?;
            SigningKey::from_bytes(&key_array)
        } else {
            info!("Generating new audit key at {:?}", key_path);
            let mut csprng = OsRng;
            let signing_key = SigningKey::generate(&mut csprng);
            fs::write(&key_path, signing_key.to_bytes()).context("Failed to save audit key")?;
            signing_key
        };

        let logger = Self {
            log_path,
            signing_key,
            node_id,
            file_lock: Mutex::new(()),
        };
        
        info!("AuditLogger initialized. Public Key: {}", logger.public_key_hex());
        
        Ok(logger)
    }

    /// Log a signed event
    pub fn log_event(
        &self,
        action: &str,
        gpu_uuid: Option<&str>,
        status: &str,
        details: Option<String>,
    ) -> Result<()> {
        let mut record = AuditRecord {
            timestamp: Utc::now().to_rfc3339(),
            node_id: self.node_id.clone(),
            gpu_uuid: gpu_uuid.map(|s| s.to_string()),
            action: action.to_string(),
            status: status.to_string(),
            details,
            signature: None,
        };

        // Sign the record
        let signable = record.signable_string();
        let signature = self.signing_key.sign(signable.as_bytes());
        record.signature = Some(hex::encode(signature.to_bytes()));

        // Serialize to JSON
        let json_line = serde_json::to_string(&record)?;

        // Write to file with lock
        let _lock = self.file_lock.lock().unwrap();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .context("Failed to open audit log file")?;

        writeln!(file, "{}", json_line).context("Failed to write audit log entry")?;

        Ok(())
    }

    /// Get the public key (verifying key) as hex string
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }
}
