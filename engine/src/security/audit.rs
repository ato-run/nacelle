//! Audit logging for security events

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOperation {
    CapsuleStart,
    StartCapsule,
    CapsuleStop,
    StopCapsule,
    CapsuleDelete,
    DeployCapsule,
    FileAccess,
    NetworkAccess,
    APIKeyUsed,
}

impl std::fmt::Display for AuditOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AuditOperation::CapsuleStart | AuditOperation::StartCapsule => "capsule_start",
            AuditOperation::CapsuleStop | AuditOperation::StopCapsule => "capsule_stop",
            AuditOperation::CapsuleDelete => "capsule_delete",
            AuditOperation::DeployCapsule => "deploy_capsule",
            AuditOperation::FileAccess => "file_access",
            AuditOperation::NetworkAccess => "network_access",
            AuditOperation::APIKeyUsed => "api_key_used",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditStatus {
    Success,
    Failure,
}

impl std::fmt::Display for AuditStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditStatus::Success => write!(f, "success"),
            AuditStatus::Failure => write!(f, "failure"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub operation: AuditOperation,
    pub status: AuditStatus,
    pub timestamp: u64,
    pub user_id: Option<String>,
    pub details: Option<String>,
}

pub struct AuditLogger {
    #[allow(dead_code)]
    log_path: PathBuf,
    #[allow(dead_code)]
    key_path: PathBuf,
    #[allow(dead_code)]
    node_id: String,
}

impl AuditLogger {
    /// Create a new AuditLogger with file paths and node identifier
    pub fn new(log_path: PathBuf, key_path: PathBuf, node_id: String) -> Result<Self> {
        // Ensure parent directories exist
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        Ok(Self {
            log_path,
            key_path,
            node_id,
        })
    }

    pub async fn log(&self, operation: AuditOperation, status: AuditStatus, details: Option<String>) {
        let event = AuditEvent {
            operation,
            status,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            user_id: None,
            details,
        };

        // In production: write to persistent storage
        tracing::info!("Audit: {:?}", event);
    }

    pub async fn log_event(&self, operation: AuditOperation, status: AuditStatus) {
        self.log(operation, status, None).await;
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self {
            log_path: PathBuf::from("/tmp/audit.log"),
            key_path: PathBuf::from("/tmp/node_key.pem"),
            node_id: "default-node".to_string(),
        }
    }
}
