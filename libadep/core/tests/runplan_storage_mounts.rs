use libadep_core::capsule_v1::CapsuleManifestV1;

#[test]
fn runplan_docker_storage_volumes_become_mounts() {
    std::env::set_var("GUMBALL_STORAGE_BASE", "/tmp/gumball-test/volumes");

    const MANIFEST: &str = r#"
schema_version = "1.0"
name = "hello-docker-storage"
version = "0.2.0"
type = "app"

[execution]
runtime = "docker"
entrypoint = "ghcr.io/example/hello:latest"
port = 8080

[storage]
volumes = [
  { name = "data", mount_path = "/data", read_only = false },
  { name = "models", mount_path = "/models", read_only = true },
]
"#;

    let manifest = CapsuleManifestV1::from_toml(MANIFEST).unwrap();
    manifest.validate().unwrap();
    let plan = manifest.to_run_plan().unwrap();

    let docker = match plan.runtime {
        libadep_core::runplan::RunPlanRuntime::Docker(d) => d,
        _ => panic!("expected docker runtime"),
    };

    assert_eq!(docker.mounts.len(), 2);
    assert_eq!(
        docker.mounts[0].source,
        "/tmp/gumball-test/volumes/hello-docker-storage/data"
    );
    assert_eq!(docker.mounts[0].target, "/data");
    assert!(!docker.mounts[0].readonly);
    assert_eq!(
        docker.mounts[1].source,
        "/tmp/gumball-test/volumes/hello-docker-storage/models"
    );
    assert_eq!(docker.mounts[1].target, "/models");
    assert!(docker.mounts[1].readonly);
}

#[test]
fn storage_rejected_for_non_docker() {
    const MANIFEST: &str = r#"
schema_version = "1.0"
name = "native-with-storage"
version = "0.1.0"
type = "app"

[execution]
runtime = "native"
entrypoint = "/usr/bin/true"

[storage]
volumes = [
  { name = "data", mount_path = "/data", read_only = false },
]
"#;

    let manifest = CapsuleManifestV1::from_toml(MANIFEST).unwrap();
    assert!(manifest.validate().is_err());
}
