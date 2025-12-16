use std::path::PathBuf;
#[cfg_attr(not(target_os = "linux"), allow(unused_imports))]
use tracing::{info, warn};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostMount {
    pub source: String,
    pub target: String,
    pub readonly: bool,
}

#[cfg(target_os = "linux")]
fn default_process_args() -> Vec<String> {
    vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "echo 'Hello from Capsuled OCI!' && sleep 60".to_string(),
    ]
}

#[cfg(target_os = "linux")]
fn process_args_from_command(command: Option<&[String]>) -> Vec<String> {
    match command {
        Some(cmd) if !cmd.is_empty() => cmd.to_vec(),
        _ => default_process_args(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Libcontainer error: {0}")]
    Libcontainer(String),
    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
    #[error("Not supported on this OS")]
    NotSupported,
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;
    use crate::oci::ImageManager;
    use libcontainer::container::builder::ContainerBuilder;
    use libcontainer::container::Container;
    use libcontainer::syscall::syscall::SyscallType;
    use nix::sys::signal::{self, Signal};
    use nix::unistd::Pid;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    pub struct DirectRuntime {
        root_dir: PathBuf,
        image_manager: ImageManager,
        // Map container ID to PID for supervision
        active_processes: Arc<Mutex<HashMap<String, i32>>>,
    }

    impl DirectRuntime {
        pub fn new(root_dir: PathBuf) -> Self {
            let cache_dir = root_dir.join("cache");
            let rootfs_dir = root_dir.join("rootfs");

            // Ensure directories exist
            let _ = std::fs::create_dir_all(&cache_dir);
            let _ = std::fs::create_dir_all(&rootfs_dir);

            Self {
                root_dir: root_dir.clone(),
                image_manager: ImageManager::new(cache_dir, rootfs_dir),
                active_processes: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        /// Public method to start a container, handling bundle preparation.
        pub async fn start_container(
            &self,
            container_id: &str,
            image: &str,
            command: Option<&[String]>,
            mounts: Option<&[HostMount]>,
        ) -> Result<(), RuntimeError> {
            let bundle_path = self
                .prepare_bundle(container_id, image, command, mounts)
                .await?;
            self.spawn(container_id, bundle_path).await
        }

        async fn prepare_bundle(
            &self,
            container_id: &str,
            image: &str,
            command: Option<&[String]>,
            mounts: Option<&[HostMount]>,
        ) -> Result<PathBuf, RuntimeError> {
            let bundle_path = self.root_dir.join("bundles").join(container_id);

            if !bundle_path.exists() {
                info!(
                    "Preparing bundle for {} (image: {}) at {:?}",
                    container_id, image, bundle_path
                );
                std::fs::create_dir_all(&bundle_path).map_err(RuntimeError::Io)?;

                // Determine rootfs path based on whether we have a real image
                let _rootfs_path =
                    if !image.is_empty() && image != "dummy" && image != "dummy-image" {
                        // Phase 1-C: Pull real OCI image
                        info!("  [Phase 1-C] Pulling OCI image: {}", image);
                        match self.image_manager.pull_image(image).await {
                            Ok(pulled) => {
                                info!(
                                    "  Image pulled successfully: {} layers, {} bytes",
                                    pulled.layer_count, pulled.total_size
                                );
                                // Symlink or copy rootfs to bundle
                                let bundle_rootfs = bundle_path.join("rootfs");
                                if !bundle_rootfs.exists() {
                                    // Create symlink to the pulled rootfs
                                    #[cfg(unix)]
                                    std::os::unix::fs::symlink(&pulled.rootfs_path, &bundle_rootfs)
                                        .map_err(RuntimeError::Io)?;
                                }
                                bundle_rootfs
                            }
                            Err(e) => {
                                warn!(
                                    "  Failed to pull image: {:?}, falling back to bind-mount mode",
                                    e
                                );
                                self.prepare_fallback_rootfs(&bundle_path).await?
                            }
                        }
                    } else {
                        // Phase 1-B fallback: Use bind-mounts for testing
                        info!("  [Phase 1-B Fallback] Using bind-mount mode (no real image)");
                        self.prepare_fallback_rootfs(&bundle_path).await?
                    };

                Self::write_config_json(&bundle_path, command, mounts)?;
            }

            // If the bundle already exists, refresh config.json when an explicit command is provided.
            if command.is_some() || mounts.is_some() {
                Self::write_config_json(&bundle_path, command, mounts)?;
            }

            Ok(bundle_path)
        }

        /// Fallback rootfs preparation for Phase 1-B (bind-mount mode)
        async fn prepare_fallback_rootfs(
            &self,
            bundle_path: &Path,
        ) -> Result<PathBuf, RuntimeError> {
            let rootfs_path = bundle_path.join("rootfs");
            std::fs::create_dir_all(&rootfs_path).map_err(RuntimeError::Io)?;

            // Create essential directories
            for dir in [
                "bin", "lib", "usr", "proc", "dev", "sys", "tmp", "etc", "root",
            ] {
                std::fs::create_dir_all(rootfs_path.join(dir)).map_err(RuntimeError::Io)?;
            }

            Ok(rootfs_path)
        }

        fn write_config_json(
            bundle_path: &Path,
            command: Option<&[String]>,
            mounts: Option<&[HostMount]>,
        ) -> Result<(), RuntimeError> {
            // Create minimal OCI Spec
            let mut spec = oci_spec::runtime::Spec::default();

            let argv = super::process_args_from_command(command);
            let process = oci_spec::runtime::ProcessBuilder::default()
                .args(argv)
                .cwd("/".to_string())
                .env(vec![
                    "PATH=/bin:/usr/bin:/sbin:/usr/sbin".to_string(),
                    "TERM=xterm".to_string(),
                ])
                .build()
                .map_err(|e| RuntimeError::Other(anyhow::anyhow!("Spec build error: {}", e)))?;

            spec.set_process(Some(process));

            let root = oci_spec::runtime::RootBuilder::default()
                .path("rootfs".to_string())
                .readonly(false)
                .build()
                .map_err(|e| RuntimeError::Other(anyhow::anyhow!("Spec build error: {}", e)))?;
            spec.set_root(Some(root));

            // Best-effort: ensure mount source/target exist.
            // - source: host directory under e.g. /var/lib/gumball/volumes/... (HostPath)
            // - target: directory inside rootfs
            if let Some(mounts) = mounts {
                for m in mounts {
                    if !m.source.is_empty() {
                        let _ = std::fs::create_dir_all(&m.source);
                    }
                    if !m.target.is_empty() {
                        // target is expected to be absolute ("/data"), so strip leading slash.
                        let rel = m.target.trim_start_matches('/');
                        if !rel.is_empty() {
                            let rootfs = bundle_path.join("rootfs");
                            let _ = std::fs::create_dir_all(rootfs.join(rel));
                        }
                    }
                }
            }

            let base_mounts = vec![
                oci_spec::runtime::MountBuilder::default()
                    .destination("/proc".to_string())
                    .source("proc".to_string())
                    .typ("proc".to_string())
                    .build()
                    .map_err(|e| RuntimeError::Other(anyhow::anyhow!("Spec build error: {}", e)))?,
                oci_spec::runtime::MountBuilder::default()
                    .destination("/dev".to_string())
                    .source("tmpfs".to_string())
                    .typ("tmpfs".to_string())
                    .options(vec![
                        "nosuid".to_string(),
                        "strictatime".to_string(),
                        "mode=755".to_string(),
                        "size=65536k".to_string(),
                    ])
                    .build()
                    .map_err(|e| RuntimeError::Other(anyhow::anyhow!("Spec build error: {}", e)))?,
            ];

            let mut all_mounts = base_mounts;
            if let Some(host_mounts) = mounts {
                for m in host_mounts {
                    let dest = m.target.clone();
                    if dest.is_empty() {
                        continue;
                    }
                    if all_mounts
                        .iter()
                        .any(|x| x.destination().to_str() == Some(dest.as_str()))
                    {
                        continue;
                    }

                    let mut options = vec!["rbind".to_string()];
                    if m.readonly {
                        options.push("ro".to_string());
                    } else {
                        options.push("rw".to_string());
                    }

                    all_mounts.push(
                        oci_spec::runtime::MountBuilder::default()
                            .destination(dest)
                            .source(m.source.clone())
                            .typ("bind".to_string())
                            .options(options)
                            .build()
                            .map_err(|e| {
                                RuntimeError::Other(anyhow::anyhow!("Spec build error: {}", e))
                            })?,
                    );
                }
            }

            spec.set_mounts(Some(all_mounts));
            spec.set_version("1.0.2".to_string());

            let config_path = bundle_path.join("config.json");
            let f = std::fs::File::create(&config_path).map_err(RuntimeError::Io)?;
            serde_json::to_writer_pretty(f, &spec)
                .map_err(|e| RuntimeError::Other(anyhow::anyhow!("Json write error: {}", e)))?;

            Ok(())
        }

        async fn spawn(
            &self,
            container_id: &str,
            bundle_path: PathBuf,
        ) -> Result<(), RuntimeError> {
            info!(
                "Spawning container directly: {} from {:?}",
                container_id, bundle_path
            );

            if !bundle_path.exists() {
                return Err(RuntimeError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Bundle path not found: {:?}", bundle_path),
                )));
            }

            let container_id = container_id.to_string();
            let root_dir = self.root_dir.clone();
            let active_processes = self.active_processes.clone();

            tokio::task::spawn_blocking(move || {
                let mut container = ContainerBuilder::new(container_id.clone(), SyscallType::Linux)
                    .with_root_path(root_dir)
                    .map_err(|e| {
                        RuntimeError::Libcontainer(format!("Failed to set root path: {}", e))
                    })?
                    .as_init(&bundle_path)
                    .with_systemd(false)
                    .build()
                    .map_err(|e| {
                        RuntimeError::Libcontainer(format!("Failed to build container: {}", e))
                    })?;

                container.start().map_err(|e| {
                    RuntimeError::Libcontainer(format!("Failed to start container: {}", e))
                })?;

                let pid = container.pid().ok_or_else(|| {
                    RuntimeError::Libcontainer("Failed to get PID after start".to_string())
                })?;
                info!("Container {} started with PID {}", container_id, pid);

                {
                    let mut procs = active_processes.lock().unwrap();
                    procs.insert(container_id.clone(), pid.as_raw());
                }

                // Wait for process to exit
                match nix::sys::wait::waitpid(Pid::from_raw(pid.as_raw()), None) {
                    Ok(status) => {
                        info!("Container {} exited with status {:?}", container_id, status)
                    }
                    Err(e) => warn!("Failed to wait for container {}: {}", container_id, e),
                }

                {
                    let mut procs = active_processes.lock().unwrap();
                    procs.remove(&container_id);
                }

                Ok::<(), RuntimeError>(())
            })
            .await
            .map_err(|e| RuntimeError::Other(anyhow::anyhow!("JoinError: {}", e)))??;

            Ok(())
        }

        pub async fn delete(&self, container_id: &str) -> Result<(), RuntimeError> {
            let container_id = container_id.to_string();
            let root_dir = self.root_dir.clone();
            let active_processes = self.active_processes.clone();

            tokio::task::spawn_blocking(move || {
                // If process is tracked, kill it first
                let pid = {
                    let procs = active_processes.lock().unwrap();
                    procs.get(&container_id).cloned()
                };

                if let Some(pid) = pid {
                    info!("Killing container process {}", pid);
                    let _ = signal::kill(Pid::from_raw(pid), Signal::SIGKILL);
                }

                // For deletion, load existing container state
                let mut container = Container::load(root_dir.join(&container_id)).map_err(|e| {
                    RuntimeError::Libcontainer(format!(
                        "Failed to load container for deletion: {}",
                        e
                    ))
                })?;

                match container.delete(true) {
                    Ok(_) => info!("Container {} deleted", container_id),
                    Err(e) => warn!("Failed to delete container {}: {}", container_id, e),
                }

                Ok::<(), RuntimeError>(())
            })
            .await
            .map_err(|e| RuntimeError::Other(anyhow::anyhow!("JoinError: {}", e)))??;

            Ok(())
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod other_impl {
    use super::*;

    pub struct DirectRuntime {
        #[allow(dead_code)]
        root_dir: PathBuf,
    }

    impl DirectRuntime {
        pub fn new(root_dir: PathBuf) -> Self {
            Self { root_dir }
        }

        pub async fn start_container(
            &self,
            _container_id: &str,
            _image: &str,
            _command: Option<&[String]>,
            _mounts: Option<&[HostMount]>,
        ) -> Result<(), RuntimeError> {
            warn!("DirectRuntime::start_container is not supported on non-Linux OS");
            Err(RuntimeError::NotSupported)
        }

        pub async fn spawn(
            &self,
            _container_id: &str,
            _bundle_path: PathBuf,
        ) -> Result<(), RuntimeError> {
            warn!("DirectRuntime::spawn is not supported on non-Linux OS");
            Err(RuntimeError::NotSupported)
        }

        pub async fn delete(&self, _container_id: &str) -> Result<(), RuntimeError> {
            warn!("DirectRuntime::delete is not supported on non-Linux OS");
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_impl::DirectRuntime;

#[cfg(not(target_os = "linux"))]
pub use other_impl::DirectRuntime;
