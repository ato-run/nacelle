use libadep::core::error::AdepError;
use serde_json::json;

pub fn unsupported_type(t: &str) -> AdepError {
    AdepError::new(
        "ADEP-RUNTIME-UNSUPPORTED-TYPE",
        format!("Runtime type '{t}' is not supported"),
    )
    .with_hint("Supported types in Week 4: container")
    .with_docs("https://docs.adep.dev/runtime-types")
}

pub fn unsupported_engine(engine: &str) -> AdepError {
    AdepError::new(
        "ADEP-ENGINE-UNSUPPORTED",
        format!("Engine '{engine}' is not supported"),
    )
    .with_hint("Supported engines: podman, auto")
    .with_docs("https://docs.adep.dev/runtime-engines")
}

pub fn docker_direct_not_yet() -> AdepError {
    AdepError::new(
        "ADEP-DOCKER-DIRECT-NOT-YET",
        "Direct docker engine is not yet supported",
    )
    .with_hint(
        "Use 'engine: \"auto\"' to auto-detect podman or docker. Direct support lands Week 5.",
    )
    .with_docs("https://docs.adep.dev/runtime-engines")
}

pub fn no_container_engine() -> AdepError {
    AdepError::new(
        "ADEP-ENGINE-NOTFOUND",
        "No container engine found (podman or docker required)",
    )
    .with_hint("Install podman (recommended) or docker to run containerized apps.")
    .with_docs("https://docs.adep.dev/install-engines")
    .with_context(json!({
        "fix_command": "brew install podman && podman machine init && podman machine start"
    }))
}

pub fn podman_required() -> AdepError {
    AdepError::new(
        "ADEP-PODMAN-REQUIRED",
        "Podman is required for 'engine: \"podman\"'",
    )
    .with_hint("Install podman, or change to 'engine: \"auto\"' to allow docker fallback.")
    .with_docs("https://docs.adep.dev/install-podman")
    .with_context(json!({
        "fix_command": "brew install podman && podman machine init && podman machine start"
    }))
}

pub fn unsupported_language(lang: &str) -> AdepError {
    AdepError::new(
        "ADEP-LANGUAGE-UNSUPPORTED",
        format!("Language '{lang}' is not supported"),
    )
    .with_hint("Supported languages in Week 4: python (3.9-3.12)")
    .with_docs("https://docs.adep.dev/supported-languages")
}

pub fn unsupported_version(lang: &str, ver: &str) -> AdepError {
    AdepError::new(
        "ADEP-VERSION-UNSUPPORTED",
        format!("{lang} version '{ver}' is not supported"),
    )
    .with_hint("Supported Python versions: 3.9, 3.10, 3.11, 3.12")
    .with_docs("https://docs.adep.dev/supported-versions")
}

pub fn no_runtime_specified() -> AdepError {
    AdepError::new(
        "ADEP-RUNTIME-MISSING",
        "No runtime specified in manifest.json",
    )
    .with_hint(
        "Add a 'runtime' section for dynamic apps or use 'adep run --static' for static apps.",
    )
    .with_docs("https://docs.adep.dev/manifest-runtime")
}

pub fn home_dir_unavailable() -> AdepError {
    AdepError::new(
        "ADEP-HOME-NOTFOUND",
        "Unable to resolve HOME directory for user-scoped cache",
    )
    .with_hint("Set the HOME environment variable before running adep.")
    .with_docs("https://docs.adep.dev/runtime-environment")
}

pub fn audit_proxy_failed(details: String) -> AdepError {
    AdepError::new(
        "ADEP-AUDIT-PROXY-FAILED",
        format!("Audit proxy failed: {details}"),
    )
    .with_hint("Ensure the audit log directory is writable and required ports are available.")
    .with_docs("https://docs.adep.dev/audit-logging")
}

pub fn dev_mode_blocked() -> AdepError {
    AdepError::new(
        "ADEP-DEV-MODE-BLOCKED",
        "egress_mode: \"dev\" requires explicit permission",
    )
    .with_hint(
        "Development only! Enable temporarily with: export ADEP_ALLOW_DEV_MODE=1 (never in production)",
    )
    .with_docs("https://docs.adep.dev/dev-mode")
    .with_context(json!({ "fix_command": "export ADEP_ALLOW_DEV_MODE=1" }))
}

pub fn dependency_not_running(name: &str) -> AdepError {
    AdepError::new(
        "ADEP-DEP-NOT-RUNNING",
        format!("Required ADEP '{name}' is not running"),
    )
    .with_hint("Start it first from its repository")
    .with_docs("https://docs.adep.dev/multi-adep")
}

pub fn compose_dev_only() -> AdepError {
    AdepError::new(
        "ADEP-COMPOSE-DEV-ONLY",
        "'adep compose' is a development tool and requires ADEP_ALLOW_DEV_MODE=1",
    )
    .with_hint("Set export ADEP_ALLOW_DEV_MODE=1 for development-only usage.")
    .with_docs("https://docs.adep.dev/compose")
    .with_context(json!({ "fix_command": "export ADEP_ALLOW_DEV_MODE=1" }))
}
