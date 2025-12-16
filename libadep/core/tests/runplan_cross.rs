use std::path::PathBuf;
use std::process::Command;

use libadep_core::capsule_v1::CapsuleManifestV1;

fn go_runplan_output(manifest: &str) -> serde_json::Value {
    let tmp = tempfile::NamedTempFile::new().expect("tmp file");
    std::fs::write(tmp.path(), manifest).expect("write manifest");

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("libadep dir")
        .parent()
        .expect("onescluster root")
        .to_path_buf();
    // Go module root lives at client (module: github.com/onescluster/coordinator)
    let go_workdir = repo_root.join("client");

    let output = Command::new("go")
        .arg("run")
        .arg("./cmd/runplan")
        .arg(tmp.path())
        .current_dir(&go_workdir)
        .output()
        .expect("failed to run go");

    if !output.status.success() {
        panic!(
            "go run failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    serde_json::from_slice(&output.stdout).expect("parse go json")
}

fn rust_runplan_output(manifest: &str) -> serde_json::Value {
    let manifest = CapsuleManifestV1::from_toml(manifest).expect("parse toml");
    manifest.validate().expect("validate manifest");
    let plan = manifest.to_run_plan().expect("runplan conversion");
    serde_json::to_value(plan).expect("serialize runplan")
}

#[test]
fn runplan_python_uv_matches_go() {
    const MANIFEST: &str = r#"
schema_version = "1.0"
name = "mlx-qwen3-8b"
version = "1.0.0"
type = "inference"

[capabilities]
chat = true

[model]
source = "dummy:model"

[execution]
runtime = "python-uv"
entrypoint = "server.py"
port = 8081

[execution.env]
GUMBALL_MODEL = "qwen3-8b"
    "#;

    let go_json = go_runplan_output(MANIFEST);
    let rust_json = rust_runplan_output(MANIFEST);

    assert_eq!(go_json, rust_json);
}

#[test]
fn runplan_docker_matches_go() {
    const MANIFEST: &str = r#"
schema_version = "1.0"
name = "hello-docker"
version = "0.1.0"
type = "app"

[execution]
runtime = "docker"
entrypoint = "ghcr.io/example/hello:latest"
port = 8080
    "#;

    let go_json = go_runplan_output(MANIFEST);
    let rust_json = rust_runplan_output(MANIFEST);

    assert_eq!(go_json, rust_json);
}

#[test]
fn runplan_docker_ports_mounts_env_matches_go() {
    const MANIFEST: &str = r#"
schema_version = "1.0"
name = "hello-docker-extended"
version = "0.2.0"
type = "app"

[execution]
runtime = "docker"
entrypoint = "ghcr.io/example/hello:latest"
port = 3000

[execution.env]
FOO = "bar"
BAZ = "qux"

[execution.mounts]
mounts = [
  { source = "/host/data", target = "/app/data", readonly = true },
  { source = "/host/tmp",  target = "/app/tmp",  readonly = false },
]

    "#;

    let go_json = go_runplan_output(MANIFEST);
    let rust_json = rust_runplan_output(MANIFEST);

    assert_eq!(go_json, rust_json);
}
