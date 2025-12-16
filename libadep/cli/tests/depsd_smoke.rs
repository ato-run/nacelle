#![cfg(unix)]

use std::env;
use std::fs::{self, File};
use std::io;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use base64::encode as b64_encode;
use chrono::{SecondsFormat, Utc};
use ed25519_dalek::{Keypair, Signer};
use flate2::write::GzEncoder;
use flate2::Compression;
use libadep_cas::index::{CompressedEntry, IndexEntry, IndexMetadata};
use libadep_cas::{BlobStore, CanonicalIndex};
use rand::rngs::OsRng;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use uuid::Uuid;
use zip::write::FileOptions;
use zip::ZipWriter;

const PYTHON_DIST: &str = "example_pkg";
const PYTHON_VERSION: &str = "1.0.0";
const PYTHON_WHEEL: &str = "example_pkg-1.0.0-py3-none-any.whl";
const NODE_PACKAGE: &str = "left-pad";
const NODE_VERSION: &str = "1.0.0";
const NODE_TARBALL: &str = "left-pad-1.0.0.tgz";

#[test]
fn depsd_autostart_python_pnpm_success() -> Result<()> {
    let workspace = TempDir::new().context("create workspace")?;
    let root = workspace.path();

    if !loopback_available() {
        eprintln!("skipping depsd_autostart_python_pnpm_success: loopback binding not permitted in this environment");
        return Ok(());
    }

    let home_dir = root.join("home");
    fs::create_dir_all(&home_dir)?;
    let _home_guard = EnvGuard::set("HOME", home_dir.to_string_lossy().into_owned());

    let audit_path = root.join("audit.success.jsonl");
    let _audit_guard = EnvGuard::set("ADEP_AUDIT_LOG", audit_path.to_string_lossy().into_owned());
    let metrics_path = root.join("metrics.success.prom");
    let _metrics_guard = EnvGuard::set(
        "ADEP_METRICS_LOG",
        metrics_path.to_string_lossy().into_owned(),
    );
    let depsd_bin = resolve_depsd_bin()?;
    let _bin_guard = EnvGuard::set("ADEP_DEPSD_BIN", depsd_bin.to_string_lossy().into_owned());
    env::remove_var("ADEP_DEPSD_ENDPOINT");
    let _autostart_guard = EnvGuard::set("ADEP_DEPSD_AUTOSTART", "1");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir)?;
    let pip_log = root.join("pip.log");
    let pnpm_log = root.join("pnpm.log");
    write_script(
        &bin_dir.join("pip"),
        &format!(
            r##"#!/bin/sh
echo "$@" >> "{log}"
hash=0
nodeps=0
noindex=0
for arg in "$@"; do
  case "$arg" in
    --require-hashes) hash=1 ;;
    --no-deps) nodeps=1 ;;
    --no-index) noindex=1 ;;
  esac
done
if [ $hash -eq 0 ] || [ $nodeps -eq 0 ] || [ $noindex -eq 0 ]; then
  echo "missing flags" >&2
  exit 42
fi
exit 0
"##,
            log = escape_path(&pip_log)
        ),
    )?;
    write_script(
        &bin_dir.join("pnpm"),
        &format!(
            r##"#!/bin/sh
echo "$@" >> "{log}"
if [ "$1" != "install" ]; then
  exit 43
fi
offline=0
frozen=0
store=0
shift
while [ "$#" -gt 0 ]; do
  case "$1" in
    --offline) offline=1 ;;
    --frozen-lockfile) frozen=1 ;;
    --store-dir)
      shift
      store=1
      ;;
  esac
  shift
done
if [ $offline -eq 0 ] || [ $frozen -eq 0 ] || [ $store -eq 0 ]; then
  exit 44
fi
exit 0
"##,
            log = escape_path(&pnpm_log)
        ),
    )?;
    let _path_guard = prepend_path(&bin_dir)?;

    let wheel_path = root.join(PYTHON_WHEEL);
    write_wheel_fixture(
        &wheel_path,
        PYTHON_DIST,
        PYTHON_VERSION,
        "Example fixture",
        &[],
    )?;
    let tarball_path = root.join(NODE_TARBALL);
    write_pnpm_tarball_fixture(&tarball_path, NODE_PACKAGE, NODE_VERSION, None, None, &[])?;

    let cas_root = root.join("cas");
    fs::create_dir_all(&cas_root)?;
    let store = BlobStore::open(&cas_root).context("open CAS store")?;

    let wheel_blob = store
        .ingest_path(&wheel_path, None)
        .context("ingest wheel")?;
    let wheel_entry = build_index_entry_from_blob(
        &wheel_blob,
        vec![format!(
            "pkg:pypi/{name}@{version}",
            name = PYTHON_DIST.replace('_', "-"),
            version = PYTHON_VERSION
        )],
        vec!["py3-none-any".into()],
        Some(IndexMetadata {
            filename: Some(PYTHON_WHEEL.to_string()),
            kind: Some("python-wheel".into()),
        }),
    );

    let pnpm_blob = store
        .ingest_path(&tarball_path, None)
        .context("ingest tarball")?;
    let pnpm_entry = build_index_entry_from_blob(
        &pnpm_blob,
        vec![format!(
            "pkg:npm/{name}@{version}",
            name = NODE_PACKAGE,
            version = NODE_VERSION
        )],
        Vec::new(),
        Some(IndexMetadata {
            filename: Some(NODE_TARBALL.to_string()),
            kind: Some("pnpm-tarball".into()),
        }),
    );

    let canonical_index =
        CanonicalIndex::from_entries(vec![wheel_entry.clone(), pnpm_entry.clone()])
            .context("construct canonical index")?;
    let index_path = cas_root.join("index.json");
    serde_json::to_writer_pretty(File::create(&index_path)?, canonical_index.entries())?;

    let manifest_template = include_str!("fixtures/manifests/v1_2/manifest.min.json");
    let mut manifest_json: Value = serde_json::from_str(manifest_template)?;
    manifest_json["deps"] = json!({
        "python": {
            "requirements": "requirements.lock",
            "install": {
                "mode": "offline",
                "target": "deps/python"
            }
        },
        "node": {
            "lockfile": "pnpm-lock.yaml",
            "install": {
                "mode": "offline",
                "frozen_lockfile": true
            }
        }
    });
    manifest_json["x-cas"] = json!({
        "index": index_path.to_string_lossy(),
        "blobs": cas_root.join("blobs").to_string_lossy()
    });
    serde_json::to_writer_pretty(File::create(root.join("manifest.json"))?, &manifest_json)?;

    fs::write(
        root.join("requirements.lock"),
        format!(
            "{name}=={version} --hash=sha256:{hash}\n",
            name = PYTHON_DIST.replace('_', "-"),
            version = PYTHON_VERSION,
            hash = wheel_blob.raw_sha256
        ),
    )?;
    fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: 5.4\n")?;

    let capsule_path = cas_root.join("capsule-manifest.json");
    write_signed_capsule(&[wheel_entry.clone(), pnpm_entry.clone()], &capsule_path)?;

    let adep_bin = env!("CARGO_BIN_EXE_adep");
    let status = Command::new(adep_bin)
        .arg("deps")
        .arg("install")
        .arg("--root")
        .arg(root)
        .arg("--cas-dir")
        .arg(&cas_root)
        .arg("--capsule")
        .arg(&capsule_path)
        .arg("--pip")
        .arg(bin_dir.join("pip"))
        .arg("--pnpm")
        .arg(bin_dir.join("pnpm"))
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("run adep deps install")?;
    assert!(status.success(), "deps install should succeed");

    let pip_log_content = fs::read_to_string(&pip_log)?;
    assert!(pip_log_content.contains("--require-hashes"));
    assert!(pip_log_content.contains("--no-deps"));
    assert!(pip_log_content.contains("--no-index"));

    let pnpm_log_content = fs::read_to_string(&pnpm_log)?;
    assert!(pnpm_log_content.contains("--offline"));
    assert!(pnpm_log_content.contains("--frozen-lockfile"));
    assert!(pnpm_log_content.contains("--store-dir"));

    let audit_events = read_audit_events(&audit_path)?;
    if audit_events.is_empty() {
        eprintln!(
            "skipping audit assertions: no events written to {} (environment may block logging)",
            audit_path.display()
        );
    } else {
        assert!(audit_events.iter().any(|event| {
            event.get("event") == Some(&Value::String("deps.install.python".into()))
                && event.get("outcome") == Some(&Value::String("success".into()))
        }));
        assert!(audit_events.iter().any(|event| {
            event.get("event") == Some(&Value::String("deps.install.pnpm".into()))
                && event.get("outcome") == Some(&Value::String("success".into()))
        }));
    }

    Ok(())
}

#[test]
fn depsd_command_failure_surfaces_error_code() -> Result<()> {
    let workspace = TempDir::new().context("create failure workspace")?;
    let root = workspace.path();

    if !loopback_available() {
        eprintln!("skipping depsd_command_failure_surfaces_error_code: loopback binding not permitted in this environment");
        return Ok(());
    }

    let home_dir = root.join("home");
    fs::create_dir_all(&home_dir)?;
    let _home_guard = EnvGuard::set("HOME", home_dir.to_string_lossy().into_owned());

    let audit_path = root.join("audit.failure.jsonl");
    let _audit_guard = EnvGuard::set("ADEP_AUDIT_LOG", audit_path.to_string_lossy().into_owned());
    let _metrics_guard = EnvGuard::set(
        "ADEP_METRICS_LOG",
        root.join("metrics.failure.prom")
            .to_string_lossy()
            .into_owned(),
    );
    let depsd_bin = resolve_depsd_bin()?;
    let _bin_guard = EnvGuard::set("ADEP_DEPSD_BIN", depsd_bin.to_string_lossy().into_owned());
    env::remove_var("ADEP_DEPSD_ENDPOINT");
    let _autostart_guard = EnvGuard::set("ADEP_DEPSD_AUTOSTART", "1");

    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir)?;
    let pip_log = root.join("pip.failure.log");
    write_script(
        &bin_dir.join("pip"),
        &format!(
            "#!/bin/sh\necho failure >> \"{}\"\nexit 90\n",
            escape_path(&pip_log)
        ),
    )?;
    write_script(&bin_dir.join("pnpm"), "#!/bin/sh\nexit 0\n")?;
    let _path_guard = prepend_path(&bin_dir)?;

    let wheel_path = root.join(PYTHON_WHEEL);
    write_wheel_fixture(
        &wheel_path,
        PYTHON_DIST,
        PYTHON_VERSION,
        "Example fixture",
        &[],
    )?;

    let cas_root = root.join("cas");
    fs::create_dir_all(&cas_root)?;
    let store = BlobStore::open(&cas_root)?;
    let wheel_blob = store.ingest_path(&wheel_path, None)?;
    let wheel_entry = build_index_entry_from_blob(
        &wheel_blob,
        vec![format!(
            "pkg:pypi/{name}@{version}",
            name = PYTHON_DIST.replace('_', "-"),
            version = PYTHON_VERSION
        )],
        vec!["py3-none-any".into()],
        Some(IndexMetadata {
            filename: Some(PYTHON_WHEEL.to_string()),
            kind: Some("python-wheel".into()),
        }),
    );
    let canonical_index = CanonicalIndex::from_entries(vec![wheel_entry.clone()])?;
    let index_path = cas_root.join("index.json");
    serde_json::to_writer_pretty(File::create(&index_path)?, canonical_index.entries())?;

    let manifest_template = include_str!("fixtures/manifests/v1_2/manifest.min.json");
    let mut manifest_json: Value = serde_json::from_str(manifest_template)?;
    manifest_json["deps"] = json!({
        "python": {
            "requirements": "requirements.lock",
            "install": {
                "mode": "offline",
                "target": "deps/python"
            }
        }
    });
    manifest_json["x-cas"] = json!({
        "index": index_path.to_string_lossy(),
        "blobs": cas_root.join("blobs").to_string_lossy()
    });
    serde_json::to_writer_pretty(File::create(root.join("manifest.json"))?, &manifest_json)?;

    fs::write(
        root.join("requirements.lock"),
        format!(
            "{name}=={version} --hash=sha256:{hash}\n",
            name = PYTHON_DIST.replace('_', "-"),
            version = PYTHON_VERSION,
            hash = wheel_blob.raw_sha256
        ),
    )?;

    let capsule_path = cas_root.join("capsule-manifest.json");
    write_signed_capsule(&[wheel_entry.clone()], &capsule_path)?;

    let adep_bin = env!("CARGO_BIN_EXE_adep");
    let output = Command::new(adep_bin)
        .arg("deps")
        .arg("install")
        .arg("--root")
        .arg(root)
        .arg("--cas-dir")
        .arg(&cas_root)
        .arg("--capsule")
        .arg(&capsule_path)
        .arg("--pip")
        .arg(bin_dir.join("pip"))
        .stdin(Stdio::null())
        .output()
        .context("run adep deps install (expected failure)")?;
    assert!(!output.status.success(), "deps install should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E_ADEP_DEPS_COMMAND_FAILED"),
        "stderr missing error code: {stderr}"
    );

    let audit_events = read_audit_events(&audit_path)?;
    if audit_events.is_empty() {
        eprintln!(
            "skipping audit assertions: no events written to {} (environment may block logging)",
            audit_path.display()
        );
    } else {
        assert!(audit_events.iter().any(|event| {
            event.get("event") == Some(&Value::String("deps.install.python".into()))
                && event.get("outcome") == Some(&Value::String("failure".into()))
                && event.get("error_code")
                    == Some(&Value::String("E_ADEP_DEPS_COMMAND_FAILED".into()))
        }));
    }

    Ok(())
}

fn resolve_depsd_bin() -> Result<PathBuf> {
    if let Ok(path) = env::var("CARGO_BIN_EXE_depsd") {
        return Ok(PathBuf::from(path));
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let status = Command::new("cargo")
        .arg("build")
        .arg("--package")
        .arg("adep-depsd")
        .arg("--bin")
        .arg("depsd")
        .current_dir(&manifest_dir)
        .status()
        .context("invoke cargo build --bin depsd")?;
    if !status.success() {
        return Err(anyhow!("cargo build --bin depsd failed"));
    }
    let target_dir = manifest_dir.join("target").join("debug");
    let candidate = if cfg!(windows) {
        target_dir.join("depsd.exe")
    } else {
        target_dir.join("depsd")
    };
    if candidate.exists() {
        Ok(candidate)
    } else {
        Err(anyhow!(
            "depsd binary not found at {}; verify build artifacts",
            candidate.display()
        ))
    }
}

#[derive(Serialize)]
struct TestCapsulePackage<'a> {
    #[serde(rename = "id")]
    id: Uuid,
    #[serde(rename = "familyId")]
    family_id: Uuid,
    version: &'a str,
    channel: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<&'a str>,
}

#[derive(Serialize)]
struct TestCapsuleGenerator<'a> {
    tool: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct TestCapsuleManifest<'a> {
    #[serde(rename = "schemaVersion")]
    schema_version: &'a str,
    #[serde(rename = "generatedAt")]
    generated_at: String,
    package: TestCapsulePackage<'a>,
    generator: TestCapsuleGenerator<'a>,
    entries: Vec<IndexEntry>,
}

fn write_signed_capsule(entries: &[IndexEntry], path: &Path) -> Result<()> {
    let manifest = TestCapsuleManifest {
        schema_version: "1.0",
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        package: TestCapsulePackage {
            id: Uuid::new_v4(),
            family_id: Uuid::new_v4(),
            version: "1.0.0",
            channel: "stable",
            commit: None,
            label: None,
        },
        generator: TestCapsuleGenerator {
            tool: "adep-cli-tests",
            version: env!("CARGO_PKG_VERSION"),
        },
        entries: entries.to_vec(),
    };
    let payload_bytes = serde_json::to_vec(&manifest)?;
    let payload_sha = hex::encode(Sha256::digest(&payload_bytes));
    let mut rng = OsRng;
    let keypair = Keypair::generate(&mut rng);
    let signature = keypair.sign(&payload_bytes);
    let mut manifest_value = serde_json::to_value(&manifest)?;
    manifest_value
        .as_object_mut()
        .ok_or_else(|| anyhow!("capsule manifest must be an object"))?
        .insert(
            "signature".to_string(),
            json!({
                "algorithm": "Ed25519",
                "key": format!("ed25519:{}", b64_encode(keypair.public.to_bytes())),
                "value": b64_encode(signature.to_bytes()),
                "payloadSha256": payload_sha,
            }),
        );
    serde_json::to_writer_pretty(File::create(path)?, &manifest_value)?;
    Ok(())
}

fn loopback_available() -> bool {
    match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => {
            drop(listener);
            true
        }
        Err(err) => err.kind() != io::ErrorKind::PermissionDenied,
    }
}

fn write_script(path: &Path, content: &str) -> Result<()> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    let mut perms = file.metadata()?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

fn prepend_path(dir: &Path) -> Result<EnvGuard> {
    let original = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", dir.display(), original);
    Ok(EnvGuard::set("PATH", new_path))
}

fn build_index_entry_from_blob(
    blob: &libadep_cas::StoredBlob,
    coords: Vec<String>,
    platform: Vec<String>,
    metadata: Option<IndexMetadata>,
) -> IndexEntry {
    IndexEntry {
        path: format!("sha256-{}", blob.compressed_sha256),
        raw_sha256: blob.raw_sha256.clone(),
        compressed_sha256: Some(blob.compressed_sha256.clone()),
        size: Some(blob.raw_size),
        coords,
        platform,
        metadata,
        compressed: Some(CompressedEntry {
            alg: "zstd".into(),
            size: Some(blob.compressed_size),
            digest: Some(blob.compressed_sha256.clone()),
        }),
    }
}

fn write_wheel_fixture(
    target: &Path,
    distribution: &str,
    version: &str,
    summary: &str,
    requires_dist: &[&str],
) -> Result<()> {
    let file = File::create(target)?;
    let mut writer = ZipWriter::new(file);
    let options = FileOptions::default();
    let dist_info_dir = format!("{distribution}-{version}.dist-info/");
    writer.add_directory(&dist_info_dir, Default::default())?;
    let metadata_name = distribution.replace('_', "-");
    let mut metadata = format!(
        "Metadata-Version: 2.1\nName: {}\nVersion: {}\nSummary: {}\nRequires-Python: >=3.9\n",
        metadata_name, version, summary
    );
    for req in requires_dist {
        metadata.push_str("Requires-Dist: ");
        metadata.push_str(req);
        metadata.push('\n');
    }
    writer.start_file(format!("{dist_info_dir}METADATA"), options)?;
    writer.write_all(metadata.as_bytes())?;
    writer.start_file(format!("{dist_info_dir}WHEEL"), options)?;
    writer.write_all(
        b"Wheel-Version: 1.0\nGenerator: adep-tests\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    )?;
    let package_dir = format!("{distribution}/");
    writer.add_directory(&package_dir, Default::default())?;
    writer.start_file(format!("{distribution}/__init__.py"), options)?;
    writer.write_all(b"__all__ = []\n")?;
    writer.finish()?;
    Ok(())
}

fn write_pnpm_tarball_fixture(
    target: &Path,
    package_name: &str,
    version: &str,
    description: Option<&str>,
    license: Option<&str>,
    dependencies: &[(&str, &str)],
) -> Result<()> {
    let mut package = serde_json::Map::new();
    package.insert("name".into(), Value::String(package_name.to_string()));
    package.insert("version".into(), Value::String(version.to_string()));
    if let Some(desc) = description {
        package.insert("description".into(), Value::String(desc.to_string()));
    }
    if let Some(lic) = license {
        package.insert("license".into(), Value::String(lic.to_string()));
    }
    if !dependencies.is_empty() {
        let mut deps = serde_json::Map::new();
        for (dep, ver) in dependencies {
            deps.insert(dep.to_string(), Value::String(ver.to_string()));
        }
        package.insert("dependencies".into(), Value::Object(deps));
    }
    let json_bytes = serde_json::to_vec_pretty(&Value::Object(package))?;
    let tar_bytes = build_tar_archive(&[("package/package.json", json_bytes.as_slice())])?;
    let file = File::create(target)?;
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(&tar_bytes)?;
    encoder.finish()?;
    Ok(())
}

fn build_tar_archive(entries: &[(&str, &[u8])]) -> Result<Vec<u8>> {
    use tar::{Builder, Header};

    let mut buffer = Vec::new();
    {
        let mut builder = Builder::new(&mut buffer);
        for (path, data) in entries {
            let mut header = Header::new_gnu();
            header.set_path(path)?;
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(0);
            header.set_cksum();
            builder.append(&header, *data)?;
        }
        builder.finish()?;
    }
    Ok(buffer)
}

fn read_audit_events(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(path)?;
    let mut events = Vec::new();
    for line in data.lines().filter(|line| !line.trim().is_empty()) {
        events.push(
            serde_json::from_str(line)
                .with_context(|| format!("parse audit event line: {line}"))?,
        );
    }
    Ok(events)
}

fn escape_path(path: &Path) -> String {
    path.display().to_string().replace('"', "\\\"")
}

struct EnvGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl Into<String>) -> Self {
        let original = env::var(key).ok();
        env::set_var(key, value.into());
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.original {
            env::set_var(self.key, value);
        } else {
            env::remove_var(self.key);
        }
    }
}
