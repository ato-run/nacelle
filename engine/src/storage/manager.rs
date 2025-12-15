//! Unified Storage Manager for Capsule workloads
//!
//! Combines LVM volume management and LUKS encryption into a single
//! interface for provisioning and managing capsule storage.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::storage::error::{StorageError, StorageResult};
use crate::storage::luks::{KeyStorage, LuksManager};
use crate::storage::lvm::LvmManager;

/// Information about provisioned capsule storage
#[derive(Debug, Clone)]
pub struct CapsuleStorage {
    /// Capsule ID this storage belongs to
    pub capsule_id: String,
    /// Volume group name
    pub vg_name: String,
    /// Logical volume name
    pub lv_name: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// Path to the device (raw or encrypted mapper)
    pub device_path: String,
    /// Whether the storage is encrypted
    pub encrypted: bool,
    /// Mount point (if mounted)
    pub mount_point: Option<PathBuf>,
}

/// Configuration for the StorageManager
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    /// Whether storage is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Default volume group to use
    pub default_vg: String,
    /// Directory for storing encryption keys
    pub key_directory: PathBuf,
    /// Whether to enable encryption by default
    pub enable_encryption: bool,
    /// Default volume size in bytes (for capsules that don't specify)
    pub default_size_bytes: u64,
    /// Mount point base directory
    pub mount_base: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_vg: "vg_capsules".to_string(),
            key_directory: PathBuf::from("/etc/capsuled/keys"),
            enable_encryption: true,
            default_size_bytes: 10 * 1024 * 1024 * 1024, // 10GB
            mount_base: PathBuf::from("/var/lib/capsuled/mounts"),
        }
    }
}

/// Unified storage manager combining LVM and LUKS operations
pub struct StorageManager {
    lvm: LvmManager,
    luks: LuksManager,
    config: StorageConfig,
}

impl StorageManager {
    /// Create a new StorageManager with the given configuration
    pub fn new(config: StorageConfig) -> Self {
        let lvm = LvmManager::new(config.default_vg.clone());
        let luks = LuksManager::new(config.key_directory.clone());
        
        Self { lvm, luks, config }
    }
    
    /// Create a StorageManager with default configuration
    pub fn with_defaults() -> Self {
        Self::new(StorageConfig::default())
    }

    /// Whether storage is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
    
    /// Provision storage for a capsule
    ///
    /// This creates an LVM volume and optionally encrypts it with LUKS.
    ///
    /// # Arguments
    /// * `capsule_id` - Unique identifier for the capsule
    /// * `size_bytes` - Size of storage to provision (uses default if None)
    /// * `encrypt` - Whether to encrypt (uses config default if None)
    ///
    /// # Returns
    /// CapsuleStorage with provisioned storage information
    pub fn provision_capsule_storage(
        &self,
        capsule_id: &str,
        size_bytes: Option<u64>,
        encrypt: Option<bool>,
    ) -> StorageResult<CapsuleStorage> {
        let size = size_bytes.unwrap_or(self.config.default_size_bytes);
        let should_encrypt = encrypt.unwrap_or(self.config.enable_encryption);
        
        // Generate LV name from capsule ID (sanitized)
        let lv_name = Self::sanitize_lv_name(capsule_id);
        let mount_point = self.config.mount_base.join(&lv_name);
        
        info!(
            capsule_id = capsule_id,
            lv_name = %lv_name,
            size_bytes = size,
            encrypted = should_encrypt,
            "Provisioning capsule storage"
        );
        
        // Step 1: Create LVM volume
        let volume = self.lvm.create_volume(&lv_name, size, None)?;
        debug!("Created LVM volume: {}", volume.device_path);
        
        // Step 2: Encrypt if required
        let (device_path, encrypted) = if should_encrypt {
            match self.encrypt_volume(&volume.device_path, &lv_name) {
                Ok(mapper_path) => (mapper_path, true),
                Err(e) => {
                    // Cleanup LVM volume on encryption failure
                    error!("Encryption failed, cleaning up LVM volume: {}", e);
                    let _ = self.lvm.delete_volume(&lv_name, None);
                    return Err(e);
                }
            }
        } else {
            // Format and mount unencrypted volume
            self.mount_device(&volume.device_path, &mount_point)?;
            (volume.device_path.clone(), false)
        };
        
        info!(
            capsule_id = capsule_id,
            device_path = %device_path,
            "Capsule storage provisioned successfully"
        );
        
        Ok(CapsuleStorage {
            capsule_id: capsule_id.to_string(),
            vg_name: volume.vg_name,
            lv_name,
            size_bytes: size,
            device_path,
            encrypted,
            mount_point: Some(mount_point),
        })
    }
    
    /// Cleanup storage for a capsule
    ///
    /// This locks any LUKS encryption and deletes the LVM volume.
    ///
    /// # Arguments
    /// * `capsule_id` - Unique identifier for the capsule
    pub fn cleanup_capsule_storage(&self, capsule_id: &str) -> StorageResult<()> {
        let lv_name = Self::sanitize_lv_name(capsule_id);
        let mapper_name = format!("capsule_{}", lv_name);
        let mount_point = self.config.mount_base.join(&lv_name);
        
        info!(capsule_id = capsule_id, "Cleaning up capsule storage");

        // Unmount if mounted
        if mount_point.exists() {
            debug!("Unmounting {}", mount_point.display());
            if let Err(e) = self.unmount_device(&mount_point) {
                warn!("Failed to unmount {}: {}", mount_point.display(), e);
            }
        }
        
        // Step 1: Lock LUKS volume if it exists
        let mapper_path = format!("/dev/mapper/{}", mapper_name);
        if std::path::Path::new(&mapper_path).exists() {
            debug!("Locking LUKS volume: {}", mapper_name);
            match self.luks.lock_volume(&mapper_name) {
                Ok(_) => debug!("LUKS volume locked"),
                Err(e) => warn!("Failed to lock LUKS volume (may already be locked): {}", e),
            }
        }

        // Delete key file if present
        let key_path = self.config.key_directory.join(format!("{}.key", mapper_name));
        if key_path.exists() {
            let _ = fs::remove_file(&key_path);
        }
        
        // Step 2: Delete LVM volume
        debug!("Deleting LVM volume: {}", lv_name);
        match self.lvm.delete_volume(&lv_name, None) {
            Ok(_) => {
                info!(capsule_id = capsule_id, "Capsule storage cleaned up successfully");
                Ok(())
            }
            Err(e) => {
                // Volume might not exist (already cleaned up)
                if matches!(e, StorageError::VolumeNotFound(_)) {
                    warn!("Volume not found during cleanup (may already be deleted)");
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }
    
    /// Check if storage exists for a capsule
    pub fn storage_exists(&self, capsule_id: &str) -> StorageResult<bool> {
        let lv_name = Self::sanitize_lv_name(capsule_id);
        let volumes = self.lvm.list_volumes(None)?;
        Ok(volumes.iter().any(|v| v.lv_name == lv_name))
    }
    
    /// Get storage information for a capsule
    pub fn get_capsule_storage(&self, capsule_id: &str) -> StorageResult<Option<CapsuleStorage>> {
        let lv_name = Self::sanitize_lv_name(capsule_id);
        let volumes = self.lvm.list_volumes(None)?;
        
        if let Some(volume) = volumes.iter().find(|v| v.lv_name == lv_name) {
            let mapper_name = format!("capsule_{}", lv_name);
            let mapper_path = format!("/dev/mapper/{}", mapper_name);
            let encrypted = std::path::Path::new(&mapper_path).exists();
            
            let device_path = if encrypted {
                mapper_path
            } else {
                volume.device_path.clone()
            };
            
            Ok(Some(CapsuleStorage {
                capsule_id: capsule_id.to_string(),
                vg_name: volume.vg_name.clone(),
                lv_name: volume.lv_name.clone(),
                size_bytes: volume.size_bytes,
                device_path,
                encrypted,
                mount_point: Some(self.config.mount_base.join(&lv_name)),
            }))
        } else {
            Ok(None)
        }
    }
    
    /// Encrypt a volume with LUKS
    fn encrypt_volume(&self, device_path: &str, lv_name: &str) -> StorageResult<String> {
        let mapper_name = format!("capsule_{}", lv_name);
        let mount_point = self.config.mount_base.join(lv_name);
        
        // Generate encryption key
        let key = self.luks.generate_key(64); // 512-bit key
        
        // Store key securely
        let key_path = self.luks.store_key(&mapper_name, &key)?;
        debug!("Stored encryption key: {:?}", key_path);
        
        // Create encrypted volume
        self.luks.create_encrypted_volume(device_path, KeyStorage::File(key_path.clone()))?;
        
        // Unlock the volume
        let info = self.luks.unlock_volume(device_path, &mapper_name, KeyStorage::File(key_path))?;

        // Format and mount the mapper device
        self.mount_device(&info.mapper_path, &mount_point)?;
        
        Ok(info.mapper_path)
    }
    
    /// Sanitize capsule ID to a valid LVM volume name
    fn sanitize_lv_name(capsule_id: &str) -> String {
        let sanitized: String = capsule_id
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        
        // Ensure it doesn't start with a hyphen
        if sanitized.starts_with('-') {
            format!("lv_{}", sanitized)
        } else {
            sanitized
        }
    }

    /// Format (ext4) and mount a device at the given mount point
    fn mount_device(&self, device_path: &str, mount_point: &Path) -> StorageResult<()> {
        fs::create_dir_all(mount_point)?;

        let try_mount = |path: &str, target: &Path| -> StorageResult<()> {
            let output = Command::new("mount")
                .arg(path)
                .arg(target)
                .output()
                .map_err(|e| StorageError::CommandFailed(format!("Failed to execute mount: {}", e)))?;

            if output.status.success() {
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("Permission denied") {
                return Err(StorageError::PermissionDenied("mount requires privileges".to_string()));
            }

            Err(StorageError::CommandFailed(format!("mount failed: {}", stderr)))
        };

        match try_mount(device_path, mount_point) {
            Ok(_) => return Ok(()),
            Err(StorageError::CommandFailed(ref msg))
                if msg.contains("wrong fs type") || msg.contains("unknown filesystem") => {}
            Err(e) => return Err(e),
        }

        let mkfs = Command::new("mkfs.ext4")
            .arg("-F")
            .arg(device_path)
            .output()
            .map_err(|e| StorageError::CommandFailed(format!("Failed to execute mkfs.ext4: {}", e)))?;

        if !mkfs.status.success() {
            let stderr = String::from_utf8_lossy(&mkfs.stderr);
            return Err(StorageError::CommandFailed(format!("mkfs.ext4 failed: {}", stderr)));
        }

        try_mount(device_path, mount_point)
    }

    /// Unmount a device from the given mount point
    fn unmount_device(&self, mount_point: &Path) -> StorageResult<()> {
        if !mount_point.exists() {
            return Ok(());
        }

        let output = Command::new("umount")
            .arg(mount_point)
            .output()
            .map_err(|e| StorageError::CommandFailed(format!("Failed to execute umount: {}", e)))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("busy") {
            return Err(StorageError::VolumeBusy(mount_point.display().to_string()));
        }
        if stderr.contains("Permission denied") {
            return Err(StorageError::PermissionDenied("umount requires privileges".to_string()));
        }

        Err(StorageError::CommandFailed(format!("umount failed: {}", stderr)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sanitize_lv_name() {
        assert_eq!(StorageManager::sanitize_lv_name("my-capsule"), "my-capsule");
        assert_eq!(StorageManager::sanitize_lv_name("my_capsule"), "my_capsule");
        assert_eq!(StorageManager::sanitize_lv_name("my capsule"), "my_capsule");
        assert_eq!(StorageManager::sanitize_lv_name("my.capsule"), "my_capsule");
        assert_eq!(StorageManager::sanitize_lv_name("-capsule"), "lv_-capsule");
    }
    
    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.default_vg, "vg_capsules");
        assert!(config.enable_encryption);
        assert_eq!(config.default_size_bytes, 10 * 1024 * 1024 * 1024);
    }
    
    #[test]
    fn test_capsule_storage_struct() {
        let storage = CapsuleStorage {
            capsule_id: "test-capsule".to_string(),
            vg_name: "vg_capsules".to_string(),
            lv_name: "test-capsule".to_string(),
            size_bytes: 1024 * 1024 * 1024,
            device_path: "/dev/mapper/capsule_test-capsule".to_string(),
            encrypted: true,
            mount_point: Some(PathBuf::from("/mnt/test")),
        };
        
        assert_eq!(storage.capsule_id, "test-capsule");
        assert!(storage.encrypted);
        assert_eq!(storage.mount_point.unwrap(), PathBuf::from("/mnt/test"));
    }
}
