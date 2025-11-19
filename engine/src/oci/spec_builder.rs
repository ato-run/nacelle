use crate::adep::{AdepVolume, ComputeConfig};
use oci_spec::runtime::{
    HookBuilder, HooksBuilder, Linux, LinuxBuilder, LinuxNamespaceBuilder, LinuxNamespaceType,
    Mount, MountBuilder, ProcessBuilder, RootBuilder, Spec, SpecBuilder,
};
use std::path::{Path, PathBuf};

/// Build a complete OCI runtime specification from adep.json configuration
///
/// This function is the core of Week 3's Agent implementation. It translates
/// the abstract adep.json manifest into a concrete OCI config.json that can
/// be executed by any OCI-compliant runtime (e.g., runc, crun, youki).
///
/// # GPU Support
///
/// When `requires_gpu` is true:
/// 1. Injects nvidia-container-runtime-hook as a prestart hook
/// 2. Adds NVIDIA_VISIBLE_DEVICES and NVIDIA_DRIVER_CAPABILITIES environment variables
/// 3. The hook will automatically configure GPU devices in the container
///
/// # Arguments
///
/// * `rootfs_path` - Path to the container rootfs (extracted OCI image layers)
/// * `compute` - Compute configuration from adep.json
/// * `volumes` - Volume mounts (e.g., GGUF model files)
/// * `gpu_uuids` - List of GPU UUIDs to assign (None if no GPU required)
///
/// # Returns
///
/// Complete OCI Spec ready to be serialized as config.json
///
/// # References
///
/// [11] OCI Runtime Specification: https://github.com/opencontainers/runtime-spec
/// [22] NVIDIA Container Toolkit: https://github.com/NVIDIA/nvidia-container-toolkit
/// [23] OCI Hooks: https://github.com/opencontainers/runtime-spec/blob/main/config.md#posix-platform-hooks
pub fn build_oci_spec(
    rootfs_path: &Path,
    compute: &ComputeConfig,
    volumes: &[AdepVolume],
    gpu_uuids: Option<&[String]>,
) -> Result<Spec, String> {
    // --- 1. Build Process Configuration ---
    let mut process_envs = compute.env.clone();

    // Add GPU-specific environment variables if GPU is required
    if let Some(uuids) = gpu_uuids {
        if !uuids.is_empty() {
            // NVIDIA_VISIBLE_DEVICES controls which GPUs are visible in the container
            let visible_devices = uuids.join(",");
            process_envs.push(format!("NVIDIA_VISIBLE_DEVICES={}", visible_devices));

            // NVIDIA_DRIVER_CAPABILITIES controls which driver features are enabled
            // "compute,utility" = CUDA compute + nvidia-smi utility
            process_envs.push("NVIDIA_DRIVER_CAPABILITIES=compute,utility".to_string());
        }
    }

    let process = ProcessBuilder::default()
        .args(compute.args.clone())
        .env(process_envs)
        .cwd(PathBuf::from("/"))
        .no_new_privileges(true)
        .build()
        .map_err(|e| format!("Failed to build process config: {}", e))?;

    // --- 2. Build Hooks (GPU passthrough) ---
    let hooks = if gpu_uuids.is_some() && !gpu_uuids.unwrap().is_empty() {
        // Create NVIDIA Container Toolkit prestart hook
        // This hook runs before the container starts and configures GPU devices
        let nvidia_hook = HookBuilder::default()
            .path(PathBuf::from("/usr/bin/nvidia-container-runtime-hook"))
            .args(vec![
                "nvidia-container-runtime-hook".to_string(),
                "prestart".to_string(),
            ])
            .build()
            .map_err(|e| format!("Failed to build NVIDIA hook: {}", e))?;

        Some(
            HooksBuilder::default()
                .prestart(vec![nvidia_hook])
                .build()
                .map_err(|e| format!("Failed to build hooks: {}", e))?,
        )
    } else {
        None
    };

    // --- 3. Build Mounts (default + volumes) ---
    let mut mounts = build_default_mounts();

    // Add user-specified volume mounts (e.g., GGUF model files)
    for vol in volumes {
        if vol.r#type == "bind" {
            let mut mount_options = vec!["rbind".to_string()];
            if vol.readonly {
                mount_options.push("ro".to_string());
            } else {
                mount_options.push("rw".to_string());
            }

            let mount = MountBuilder::default()
                .source(PathBuf::from(&vol.source))
                .destination(PathBuf::from(&vol.destination))
                .typ("bind".to_string())
                .options(mount_options)
                .build()
                .map_err(|e| format!("Failed to build mount for {}: {}", vol.destination, e))?;

            mounts.push(mount);
        }
    }

    // --- 4. Build Root Filesystem ---
    let root = RootBuilder::default()
        .path(rootfs_path.to_path_buf())
        .readonly(false)
        .build()
        .map_err(|e| format!("Failed to build root config: {}", e))?;

    // --- 5. Build Linux-specific Configuration ---
    let linux = build_default_linux();

    // --- 6. Assemble Final Spec ---
    let mut spec_builder = SpecBuilder::default()
        .version("1.0.2".to_string())
        .root(root)
        .process(process)
        .mounts(mounts)
        .linux(linux);

    if let Some(hooks_config) = hooks {
        spec_builder = spec_builder.hooks(hooks_config);
    }

    let spec = spec_builder
        .build()
        .map_err(|e| format!("Failed to build OCI spec: {}", e))?;

    Ok(spec)
}

/// Build default Linux container mounts
///
/// These mounts are required for basic container functionality:
/// - /proc: Process information (procfs)
/// - /dev: Device files (devtmpfs)
/// - /dev/pts: Pseudo-terminals (devpts)
/// - /sys: System information (sysfs)
fn build_default_mounts() -> Vec<Mount> {
    vec![
        // /proc (process information)
        MountBuilder::default()
            .destination(PathBuf::from("/proc"))
            .typ("proc".to_string())
            .source(PathBuf::from("proc"))
            .options(vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "nodev".to_string(),
            ])
            .build()
            .unwrap(),
        // /dev (device files)
        MountBuilder::default()
            .destination(PathBuf::from("/dev"))
            .typ("tmpfs".to_string())
            .source(PathBuf::from("tmpfs"))
            .options(vec![
                "nosuid".to_string(),
                "strictatime".to_string(),
                "mode=755".to_string(),
                "size=65536k".to_string(),
            ])
            .build()
            .unwrap(),
        // /dev/pts (pseudo-terminals)
        MountBuilder::default()
            .destination(PathBuf::from("/dev/pts"))
            .typ("devpts".to_string())
            .source(PathBuf::from("devpts"))
            .options(vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "newinstance".to_string(),
                "ptmxmode=0666".to_string(),
                "mode=0620".to_string(),
            ])
            .build()
            .unwrap(),
        // /sys (system information, read-only)
        MountBuilder::default()
            .destination(PathBuf::from("/sys"))
            .typ("sysfs".to_string())
            .source(PathBuf::from("sysfs"))
            .options(vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "nodev".to_string(),
                "ro".to_string(),
            ])
            .build()
            .unwrap(),
    ]
}

/// Build default Linux namespaces configuration
///
/// Enables container isolation using Linux namespaces:
/// - PID: Process isolation
/// - Network: Network isolation
/// - IPC: Inter-process communication isolation
/// - UTS: Hostname isolation
/// - Mount: Filesystem isolation
fn build_default_linux() -> Linux {
    let namespaces = vec![
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Pid)
            .build()
            .unwrap(),
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Network)
            .build()
            .unwrap(),
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Ipc)
            .build()
            .unwrap(),
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Uts)
            .build()
            .unwrap(),
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Mount)
            .build()
            .unwrap(),
    ];

    LinuxBuilder::default()
        .namespaces(namespaces)
        .build()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adep::{AdepVolume, ComputeConfig};

    #[test]
    fn test_spec_builder_cpu_only() {
        let compute = ComputeConfig {
            image: "hello-world".to_string(),
            args: vec!["/hello".to_string()],
            env: vec!["MY_VAR=test".to_string()],
        };
        let volumes = vec![];

        let spec = build_oci_spec(Path::new("/tmp/rootfs"), &compute, &volumes, None).unwrap();

        // 1. Verify hooks are NOT injected for CPU-only workload
        assert!(
            spec.hooks().is_none() || spec.hooks().as_ref().unwrap().prestart().is_none(),
            "CPU-only workload should not have prestart hooks"
        );

        // 2. Verify NVIDIA environment variables are NOT injected
        let envs = spec.process().as_ref().unwrap().env().as_ref().unwrap();
        assert_eq!(envs.len(), 1, "Should only have user-specified env var");
        assert_eq!(envs[0], "MY_VAR=test");
        assert!(
            !envs.iter().any(|e| e.starts_with("NVIDIA_")),
            "CPU-only workload should not have NVIDIA env vars"
        );

        // 3. Verify basic OCI structure
        assert_eq!(spec.version(), "1.0.2");
        assert!(spec.root().is_some());
        assert!(spec.process().is_some());
        assert!(spec.mounts().is_some());
        assert!(spec.linux().is_some());
    }

    #[test]
    fn test_spec_builder_with_gpu() {
        let compute = ComputeConfig {
            image: "vllm/vllm-openai".to_string(),
            args: vec!["--model".to_string(), "/models/model.gguf".to_string()],
            env: vec!["MY_VAR=test".to_string()],
        };
        let volumes = vec![];

        let gpu_uuids = vec!["GPU-1234".to_string(), "GPU-5678".to_string()];
        let spec = build_oci_spec(Path::new("/tmp/rootfs"), &compute, &volumes, Some(&gpu_uuids)).unwrap();

        // 1. Verify NVIDIA prestart hook is injected
        let hooks = spec
            .hooks()
            .as_ref()
            .expect("GPU workload should have hooks");
        let prestart = hooks
            .prestart()
            .as_ref()
            .expect("Should have prestart hooks");
        assert_eq!(prestart.len(), 1, "Should have exactly one prestart hook");
        assert_eq!(
            prestart[0].path(),
            Path::new("/usr/bin/nvidia-container-runtime-hook")
        );
        assert_eq!(prestart[0].args().as_ref().unwrap()[1], "prestart");

        // 2. Verify NVIDIA environment variables are injected
        let envs = spec.process().as_ref().unwrap().env().as_ref().unwrap();
        assert!(
            envs.iter().any(|e| e == "NVIDIA_VISIBLE_DEVICES=GPU-1234,GPU-5678"),
            "GPU workload should have NVIDIA_VISIBLE_DEVICES with UUIDs"
        );
        assert!(
            envs.iter()
                .any(|e| e == "NVIDIA_DRIVER_CAPABILITIES=compute,utility"),
            "GPU workload should have NVIDIA_DRIVER_CAPABILITIES"
        );
        assert!(
            envs.iter().any(|e| e == "MY_VAR=test"),
            "User-specified env vars should be preserved"
        );
    }

    #[test]
    fn test_spec_builder_with_volumes() {
        let compute = ComputeConfig {
            image: "vllm/vllm-openai".to_string(),
            args: vec!["--model".to_string(), "/models/model.gguf".to_string()],
            env: vec![],
        };
        let volumes = vec![
            AdepVolume {
                r#type: "bind".to_string(),
                source: "/mnt/models/llama-3.gguf".to_string(),
                destination: "/models/model.gguf".to_string(),
                readonly: true,
            },
            AdepVolume {
                r#type: "bind".to_string(),
                source: "/mnt/output".to_string(),
                destination: "/output".to_string(),
                readonly: false,
            },
        ];

        let spec = build_oci_spec(Path::new("/tmp/rootfs"), &compute, &volumes, Some(&vec!["GPU-1".to_string()])).unwrap();

        // Verify volume mounts are injected
        let mounts = spec.mounts().as_ref().unwrap();

        // Find GGUF model mount (readonly)
        let gguf_mount = mounts
            .iter()
            .find(|m| m.destination() == Path::new("/models/model.gguf"))
            .expect("GGUF mount should be present");
        assert_eq!(
            gguf_mount.source(),
            &Some(PathBuf::from("/mnt/models/llama-3.gguf"))
        );
        assert_eq!(gguf_mount.typ(), &Some("bind".to_string()));
        assert!(
            gguf_mount
                .options()
                .as_ref()
                .unwrap()
                .contains(&"ro".to_string()),
            "GGUF mount should be readonly"
        );

        // Find output mount (writable)
        let output_mount = mounts
            .iter()
            .find(|m| m.destination() == Path::new("/output"))
            .expect("Output mount should be present");
        assert_eq!(output_mount.source(), &Some(PathBuf::from("/mnt/output")));
        assert!(
            output_mount
                .options()
                .as_ref()
                .unwrap()
                .contains(&"rw".to_string()),
            "Output mount should be writable"
        );
    }

    #[test]
    fn test_default_mounts() {
        let mounts = build_default_mounts();

        // Verify essential mounts are present
        assert!(mounts.iter().any(|m| m.destination() == Path::new("/proc")));
        assert!(mounts.iter().any(|m| m.destination() == Path::new("/dev")));
        assert!(mounts
            .iter()
            .any(|m| m.destination() == Path::new("/dev/pts")));
        assert!(mounts.iter().any(|m| m.destination() == Path::new("/sys")));
    }

    #[test]
    fn test_default_linux_namespaces() {
        let linux = build_default_linux();

        // Verify essential namespaces are configured
        let namespaces = linux.namespaces().as_ref().unwrap();
        assert!(namespaces
            .iter()
            .any(|ns| ns.typ() == LinuxNamespaceType::Pid));
        assert!(namespaces
            .iter()
            .any(|ns| ns.typ() == LinuxNamespaceType::Network));
        assert!(namespaces
            .iter()
            .any(|ns| ns.typ() == LinuxNamespaceType::Mount));
    }
}
