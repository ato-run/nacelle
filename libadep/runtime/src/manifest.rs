//! Capsule manifest definitions for local execution
//!
//! This module defines a simplified manifest format optimized for local execution
//! of webcapsules in the Gumball desktop application. It is intentionally separate
//! from `libadep-core::manifest::Manifest`, which serves the broader ADEP ecosystem.
//!
//! # Design Philosophy
//!
//! The `CapsuleManifest` is designed for the MVP use case: running arbitrary commands
//! (e.g., `node server.js`, `python app.py`) in a local development environment.
//! It focuses on simplicity and direct execution, rather than the more abstract
//! platform specifications used in the core ADEP manifest.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Simplified manifest for local capsule execution
///
/// This manifest format is used by webcapsules and the Gumball desktop application.
/// It provides a straightforward way to specify how to run a capsule locally.
///
/// # Example
///
/// ```json
/// {
///   "name": "nextjs-blog",
///   "version": "1.0.0",
///   "entrypoint": {
///     "command": "node",
///     "args": ["./.webcapsule/rootfs/node_modules/.bin/next", "start", "-p", "3000"]
///   },
///   "network": {
///     "ports": [
///       { "container_port": 3000, "host_port": 3000 }
///     ]
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsuleManifest {
    /// Capsule name
    pub name: String,

    /// Capsule version (semver recommended)
    pub version: String,

    /// Entry point configuration
    pub entrypoint: Entrypoint,

    /// Network configuration (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<NetworkConfig>,
}

impl CapsuleManifest {
    /// Loads a CapsuleManifest from an adep.json file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the adep.json file
    ///
    /// # Example
    ///
    /// ```no_run
    /// use libadep_runtime::manifest::CapsuleManifest;
    /// use std::path::Path;
    ///
    /// let manifest = CapsuleManifest::load(Path::new("adep.json")).unwrap();
    /// println!("Loaded capsule: {}", manifest.name);
    /// ```
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let manifest: CapsuleManifest = serde_json::from_str(&content)?;
        Ok(manifest)
    }

    /// Saves the CapsuleManifest to a file
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the adep.json should be written
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Entrypoint configuration specifying how to start the capsule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entrypoint {
    /// The command to execute (e.g., "node", "python", "deno")
    pub command: String,

    /// Arguments to pass to the command
    ///
    /// Paths in arguments are relative to the directory containing adep.json
    pub args: Vec<String>,
}

/// Network configuration for the capsule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Port mappings between container and host
    #[serde(default)]
    pub ports: Vec<PortMapping>,
}

/// Port mapping between container and host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    /// Port inside the capsule
    pub container_port: u16,

    /// Port on the host machine
    pub host_port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_capsule_manifest() {
        let json = r#"{
            "name": "nextjs-blog",
            "version": "1.0.0",
            "entrypoint": {
                "command": "node",
                "args": ["./.webcapsule/rootfs/node_modules/.bin/next", "start", "-p", "3000"]
            },
            "network": {
                "ports": [
                    { "container_port": 3000, "host_port": 3000 }
                ]
            }
        }"#;

        let manifest: CapsuleManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "nextjs-blog");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.entrypoint.command, "node");
        assert_eq!(manifest.entrypoint.args.len(), 4);

        let network = manifest.network.unwrap();
        assert_eq!(network.ports.len(), 1);
        assert_eq!(network.ports[0].container_port, 3000);
        assert_eq!(network.ports[0].host_port, 3000);
    }

    #[test]
    fn test_manifest_without_network() {
        let json = r#"{
            "name": "simple-app",
            "version": "0.1.0",
            "entrypoint": {
                "command": "python",
                "args": ["app.py"]
            }
        }"#;

        let manifest: CapsuleManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "simple-app");
        assert!(manifest.network.is_none());
    }
}
