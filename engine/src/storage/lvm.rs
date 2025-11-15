use crate::storage::error::{StorageError, StorageResult};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::{debug, error, info};

/// Information about a logical volume
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VolumeInfo {
    /// Volume group name
    pub vg_name: String,
    /// Logical volume name
    pub lv_name: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// Device path (e.g., /dev/vg_name/lv_name)
    pub device_path: String,
    /// Whether the volume is active
    pub active: bool,
}

/// LVM (Logical Volume Manager) operations manager
///
/// Provides functionality for creating, deleting, and managing LVM volumes.
/// All operations use standard LVM command-line tools (lvcreate, lvremove, etc.)
/// without any CGO dependencies.
pub struct LvmManager {
    /// Volume group name to use for operations
    default_vg: String,
}

impl LvmManager {
    /// Creates a new LVM manager with the specified default volume group.
    ///
    /// # Arguments
    /// * `default_vg` - The default volume group name to use for operations
    ///
    /// # Example
    /// ```
    /// use capsuled_engine::storage::LvmManager;
    /// let manager = LvmManager::new("vg_data".to_string());
    /// ```
    pub fn new(default_vg: String) -> Self {
        Self { default_vg }
    }

    /// Creates a new logical volume.
    ///
    /// # Arguments
    /// * `name` - Name of the logical volume to create
    /// * `size_bytes` - Size of the volume in bytes
    /// * `vg_name` - Optional volume group name (uses default if None)
    ///
    /// # Returns
    /// VolumeInfo with the created volume information
    ///
    /// # Errors
    /// Returns StorageError if:
    /// - The volume name is invalid
    /// - The volume already exists
    /// - Insufficient space is available
    /// - The LVM command fails
    ///
    /// # Example
    /// ```no_run
    /// use capsuled_engine::storage::LvmManager;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = LvmManager::new("vg_data".to_string());
    /// let volume = manager.create_volume("my_volume", 1024 * 1024 * 1024, None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_volume(
        &self,
        name: &str,
        size_bytes: u64,
        vg_name: Option<&str>,
    ) -> StorageResult<VolumeInfo> {
        // Validate volume name (alphanumeric, underscores, hyphens only)
        if !Self::is_valid_volume_name(name) {
            return Err(StorageError::InvalidVolumeName(format!(
                "Volume name '{}' contains invalid characters",
                name
            )));
        }

        let vg = vg_name.unwrap_or(&self.default_vg);
        
        // Check if volume already exists
        if self.volume_exists(vg, name)? {
            return Err(StorageError::VolumeAlreadyExists(format!(
                "{}/{}",
                vg, name
            )));
        }

        info!(
            "Creating logical volume: {}/{} with size {} bytes",
            vg, name, size_bytes
        );

        // Convert bytes to megabytes for LVM
        let size_mb = size_bytes.div_ceil(1024 * 1024);

        // Execute lvcreate command
        let output = Command::new("lvcreate")
            .arg("-n")
            .arg(name)
            .arg("-L")
            .arg(format!("{}M", size_mb))
            .arg(vg)
            .output()
            .map_err(|e| StorageError::CommandFailed(format!("Failed to execute lvcreate: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("lvcreate failed: {}", stderr);
            
            // Check for common errors
            if stderr.contains("not found") || stderr.contains("No such") {
                return Err(StorageError::VolumeNotFound(format!(
                    "Volume group '{}' not found",
                    vg
                )));
            }
            if stderr.contains("insufficient") || stderr.contains("not enough") {
                return Err(StorageError::InsufficientSpace {
                    required: size_bytes,
                    available: 0, // Would need to query VG for actual available space
                });
            }
            
            return Err(StorageError::CommandFailed(format!(
                "lvcreate command failed: {}",
                stderr
            )));
        }

        debug!("Logical volume created successfully");

        // Build and return volume info
        Ok(VolumeInfo {
            vg_name: vg.to_string(),
            lv_name: name.to_string(),
            size_bytes,
            device_path: format!("/dev/{}/{}", vg, name),
            active: true,
        })
    }

    /// Deletes an existing logical volume.
    ///
    /// # Arguments
    /// * `name` - Name of the logical volume to delete
    /// * `vg_name` - Optional volume group name (uses default if None)
    ///
    /// # Errors
    /// Returns StorageError if:
    /// - The volume does not exist
    /// - The volume is busy
    /// - The LVM command fails
    ///
    /// # Example
    /// ```no_run
    /// use capsuled_engine::storage::LvmManager;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = LvmManager::new("vg_data".to_string());
    /// manager.delete_volume("my_volume", None)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete_volume(&self, name: &str, vg_name: Option<&str>) -> StorageResult<()> {
        let vg = vg_name.unwrap_or(&self.default_vg);
        let lv_path = format!("{}/{}", vg, name);

        // Check if volume exists
        if !self.volume_exists(vg, name)? {
            return Err(StorageError::VolumeNotFound(lv_path));
        }

        info!("Deleting logical volume: {}", lv_path);

        // Execute lvremove command with -f to force removal
        let output = Command::new("lvremove")
            .arg("-f")
            .arg(format!("/dev/{}", lv_path))
            .output()
            .map_err(|e| StorageError::CommandFailed(format!("Failed to execute lvremove: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("lvremove failed: {}", stderr);
            
            if stderr.contains("busy") || stderr.contains("in use") {
                return Err(StorageError::VolumeBusy(lv_path));
            }
            
            return Err(StorageError::CommandFailed(format!(
                "lvremove command failed: {}",
                stderr
            )));
        }

        debug!("Logical volume deleted successfully");
        Ok(())
    }

    /// Creates a snapshot of an existing logical volume.
    ///
    /// # Arguments
    /// * `source_name` - Name of the source logical volume
    /// * `snapshot_name` - Name for the snapshot
    /// * `size_bytes` - Size of the snapshot in bytes (for COW space)
    /// * `vg_name` - Optional volume group name (uses default if None)
    ///
    /// # Returns
    /// VolumeInfo with the snapshot volume information
    ///
    /// # Errors
    /// Returns StorageError if:
    /// - The source volume does not exist
    /// - The snapshot name is invalid
    /// - Insufficient space is available
    /// - The LVM command fails
    ///
    /// # Example
    /// ```no_run
    /// use capsuled_engine::storage::LvmManager;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = LvmManager::new("vg_data".to_string());
    /// let snapshot = manager.create_snapshot(
    ///     "my_volume",
    ///     "my_volume_snap",
    ///     512 * 1024 * 1024,
    ///     None
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn create_snapshot(
        &self,
        source_name: &str,
        snapshot_name: &str,
        size_bytes: u64,
        vg_name: Option<&str>,
    ) -> StorageResult<VolumeInfo> {
        // Validate snapshot name
        if !Self::is_valid_volume_name(snapshot_name) {
            return Err(StorageError::InvalidVolumeName(format!(
                "Snapshot name '{}' contains invalid characters",
                snapshot_name
            )));
        }

        let vg = vg_name.unwrap_or(&self.default_vg);
        let source_path = format!("{}/{}", vg, source_name);

        // Check if source volume exists
        if !self.volume_exists(vg, source_name)? {
            return Err(StorageError::VolumeNotFound(source_path.clone()));
        }

        // Check if snapshot already exists
        if self.volume_exists(vg, snapshot_name)? {
            return Err(StorageError::VolumeAlreadyExists(format!(
                "{}/{}",
                vg, snapshot_name
            )));
        }

        info!(
            "Creating snapshot: {}/{} from {} with size {} bytes",
            vg, snapshot_name, source_path, size_bytes
        );

        // Convert bytes to megabytes
        let size_mb = size_bytes.div_ceil(1024 * 1024);

        // Execute lvcreate for snapshot
        let output = Command::new("lvcreate")
            .arg("-s")
            .arg("-n")
            .arg(snapshot_name)
            .arg("-L")
            .arg(format!("{}M", size_mb))
            .arg(format!("/dev/{}", source_path))
            .output()
            .map_err(|e| {
                StorageError::SnapshotError(format!("Failed to execute lvcreate snapshot: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("lvcreate snapshot failed: {}", stderr);
            
            if stderr.contains("insufficient") || stderr.contains("not enough") {
                return Err(StorageError::InsufficientSpace {
                    required: size_bytes,
                    available: 0,
                });
            }
            
            return Err(StorageError::SnapshotError(format!(
                "Snapshot creation failed: {}",
                stderr
            )));
        }

        debug!("Snapshot created successfully");

        Ok(VolumeInfo {
            vg_name: vg.to_string(),
            lv_name: snapshot_name.to_string(),
            size_bytes,
            device_path: format!("/dev/{}/{}", vg, snapshot_name),
            active: true,
        })
    }

    /// Lists all logical volumes in the specified volume group.
    ///
    /// # Arguments
    /// * `vg_name` - Optional volume group name (uses default if None)
    ///
    /// # Returns
    /// Vector of VolumeInfo for all volumes in the volume group
    ///
    /// # Example
    /// ```no_run
    /// use capsuled_engine::storage::LvmManager;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = LvmManager::new("vg_data".to_string());
    /// let volumes = manager.list_volumes(None)?;
    /// for volume in volumes {
    ///     println!("Volume: {}/{}", volume.vg_name, volume.lv_name);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn list_volumes(&self, vg_name: Option<&str>) -> StorageResult<Vec<VolumeInfo>> {
        let vg = vg_name.unwrap_or(&self.default_vg);

        debug!("Listing volumes in VG: {}", vg);

        // Execute lvs command with JSON output for easier parsing
        let output = Command::new("lvs")
            .arg("--units")
            .arg("b")
            .arg("--nosuffix")
            .arg("--noheadings")
            .arg("-o")
            .arg("lv_name,lv_size,lv_active")
            .arg(vg)
            .output()
            .map_err(|e| StorageError::CommandFailed(format!("Failed to execute lvs: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("lvs failed: {}", stderr);
            
            if stderr.contains("not found") {
                return Err(StorageError::VolumeNotFound(format!(
                    "Volume group '{}' not found",
                    vg
                )));
            }
            
            return Err(StorageError::CommandFailed(format!("lvs command failed: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut volumes = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let lv_name = parts[0].trim();
                let size_bytes = parts[1]
                    .trim()
                    .parse::<u64>()
                    .map_err(|e| StorageError::ParseError(format!("Failed to parse size: {}", e)))?;
                let active = parts[2].trim() == "active";

                volumes.push(VolumeInfo {
                    vg_name: vg.to_string(),
                    lv_name: lv_name.to_string(),
                    size_bytes,
                    device_path: format!("/dev/{}/{}", vg, lv_name),
                    active,
                });
            }
        }

        debug!("Found {} volumes", volumes.len());
        Ok(volumes)
    }

    /// Checks if a logical volume exists.
    ///
    /// # Arguments
    /// * `vg_name` - Volume group name
    /// * `lv_name` - Logical volume name
    ///
    /// # Returns
    /// true if the volume exists, false otherwise
    fn volume_exists(&self, vg_name: &str, lv_name: &str) -> StorageResult<bool> {
        let output = Command::new("lvs")
            .arg(format!("{}/{}", vg_name, lv_name))
            .output()
            .map_err(|e| StorageError::CommandFailed(format!("Failed to execute lvs: {}", e)))?;

        Ok(output.status.success())
    }

    /// Validates a volume name according to LVM naming rules.
    ///
    /// Valid names contain only alphanumeric characters, underscores, and hyphens.
    fn is_valid_volume_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 128
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_lvm_manager() {
        let manager = LvmManager::new("vg_test".to_string());
        assert_eq!(manager.default_vg, "vg_test");
    }

    #[test]
    fn test_is_valid_volume_name() {
        assert!(LvmManager::is_valid_volume_name("my_volume"));
        assert!(LvmManager::is_valid_volume_name("my-volume"));
        assert!(LvmManager::is_valid_volume_name("my_volume_123"));
        assert!(LvmManager::is_valid_volume_name("MyVolume123"));
        
        assert!(!LvmManager::is_valid_volume_name(""));
        assert!(!LvmManager::is_valid_volume_name("my volume")); // space
        assert!(!LvmManager::is_valid_volume_name("my.volume")); // dot
        assert!(!LvmManager::is_valid_volume_name("my/volume")); // slash
        assert!(!LvmManager::is_valid_volume_name("my@volume")); // special char
    }

    #[test]
    fn test_volume_info_creation() {
        let info = VolumeInfo {
            vg_name: "vg_data".to_string(),
            lv_name: "test_volume".to_string(),
            size_bytes: 1024 * 1024 * 1024,
            device_path: "/dev/vg_data/test_volume".to_string(),
            active: true,
        };

        assert_eq!(info.vg_name, "vg_data");
        assert_eq!(info.lv_name, "test_volume");
        assert_eq!(info.size_bytes, 1024 * 1024 * 1024);
        assert!(info.active);
    }

    // Note: Integration tests that actually execute LVM commands
    // should be in a separate test module with proper setup/teardown
    // and should only run when LVM is available (e.g., in CI with proper permissions)
}
