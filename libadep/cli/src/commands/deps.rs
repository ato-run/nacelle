use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand, ValueEnum};
use ed25519_dalek::Verifier as _;
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer};
use flate2::read::GzDecoder;
use libadep_cas::safety::{ensure_archive_member_safe, MAX_COMPRESSION_RATIO};
use libadep_cas::{
    BlobStatus, BlobStore, CanonicalIndex, CasError, CompressedEntry, CompressedHash,
    DuplicateKind, IndexEntry, IndexMetadata, MergeConflictKind, MergeReport, StoredBlob, Verifier,
};
use libadep_deps::client::Client as DepsdClient;
use libadep_deps::proto::{
    command_log, CommandLog, InstallPnpmRequest, InstallPythonRequest, OperationError,
};
use prost::Message;
use reqwest::blocking::{Body, Client as HttpClient};
use reqwest::header::{HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use tonic::Status;
use url::Url;
use zip::ZipArchive;

use base64::{decode as base64_decode, encode as base64_encode};
use zstd::stream::Decoder as ZstdDecoder;

use crate::manifest::{Manifest, NodeDependencies, PythonDependencies};
use crate::package;
use crate::signing::StoredKey;
use uuid::Uuid;

#[derive(Args, Debug)]
pub struct DepsArgs {
    #[command(subcommand)]
    pub command: DepsCommand,
}

#[derive(Subcommand, Debug)]
pub enum DepsCommand {
    /// Verify CAS blobs against the canonical index
    Verify(VerifyArgs),
    /// Vendor dependency artifacts into the local CAS
    Vendor(VendorArgs),
    /// Generate a signed dependency capsule manifest
    Capsule(CapsuleArgs),
    /// Materialize capsule artifacts into an offline dependency cache
    Resolve(ResolveArgs),
    /// Run offline dependency installers for supported ecosystems
    Install(InstallArgs),
    /// Push a dependency capsule to an OCI-compatible registry
    Push(PushArgs),
    /// Pull a dependency capsule into the local CAS
    Pull(PullArgs),
}

#[derive(Args, Debug, Clone)]
pub struct VerifyArgs {
    /// Package root containing manifest.json
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Path to canonical index (relative to root if not absolute)
    #[arg(long)]
    pub index: Option<PathBuf>,
    /// Base directory for CAS blobs (relative to root if not absolute)
    #[arg(long)]
    pub blobs_dir: Option<PathBuf>,
    /// Output verification summary as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct VendorArgs {
    /// Language ecosystem for the artifact
    #[arg(long, value_enum)]
    pub lang: VendorLanguage,
    /// Source artifact to ingest (wheel, tarball, etc.)
    #[arg(long)]
    pub source: PathBuf,
    /// Optional coordinate identifier (pkg:pypi/... or pkg:npm/...)
    #[arg(long)]
    pub coords: Option<String>,
    /// Target platform tag
    #[arg(long)]
    pub platform: Option<String>,
    /// Override CAS root directory (defaults to ~/.adep/cas)
    #[arg(long)]
    pub cas_dir: Option<PathBuf>,
    /// Emit JSON summary instead of human-readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct CapsuleArgs {
    /// Package root containing manifest.json
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Explicit manifest path (defaults to <root>/manifest.json)
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// Path to canonical index (relative to root if not absolute)
    #[arg(long)]
    pub index: Option<PathBuf>,
    /// Base directory for CAS blobs (relative to root if not absolute)
    #[arg(long)]
    pub blobs_dir: Option<PathBuf>,
    /// Output path for the generated capsule-manifest.json
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Ed25519 key file used to sign the capsule manifest (defaults to ~/.adep/keys/deps.json)
    #[arg(long)]
    pub key: Option<PathBuf>,
    /// Emit JSON summary instead of human-readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct ResolveArgs {
    /// Package root containing manifest.json
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Explicit manifest path (defaults to <root>/manifest.json)
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// Capsule manifest path (defaults to <root>/cas/capsule-manifest.json)
    #[arg(long)]
    pub capsule: Option<PathBuf>,
    /// Base directory for CAS blobs (relative to root if not absolute)
    #[arg(long)]
    pub cas_dir: Option<PathBuf>,
    /// Directory to place resolved artifacts (defaults to <root>/deps-cache)
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Emit JSON summary
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct InstallArgs {
    /// Package root containing manifest.json
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Explicit manifest path (defaults to <root>/manifest.json)
    #[arg(long)]
    pub manifest: Option<PathBuf>,
    /// Capsule manifest path (defaults to <root>/cas/capsule-manifest.json)
    #[arg(long)]
    pub capsule: Option<PathBuf>,
    /// Base directory for CAS blobs (relative to root if not absolute)
    #[arg(long)]
    pub cas_dir: Option<PathBuf>,
    /// Directory to place resolved artifacts (defaults to <root>/deps-cache)
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Override pip binary (defaults to searching PATH)
    #[arg(long)]
    pub pip: Option<PathBuf>,
    /// Override pnpm binary (defaults to searching PATH)
    #[arg(long)]
    pub pnpm: Option<PathBuf>,
    /// Perform a dry-run and print commands without executing installers
    #[arg(long)]
    pub dry_run: bool,
    /// Emit JSON summary
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct PushArgs {
    /// Package root used to resolve relative paths
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Capsule manifest to push (defaults to <root>/cas/capsule-manifest.json)
    #[arg(long)]
    pub capsule: Option<PathBuf>,
    /// Destination registry (local OCI layout directory or remote reference)
    #[arg(long)]
    pub registry: String,
    /// OCI-style reference identifying the capsule (e.g. example.com/deps:latest)
    #[arg(long)]
    pub reference: String,
    /// Override CAS root directory containing blobs (defaults to manifest.x-cas or ~/.adep/cas)
    #[arg(long)]
    pub cas_dir: Option<PathBuf>,
    /// Emit JSON summary
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct PullArgs {
    /// Destination CAS root to populate (defaults to ~/.adep/cas)
    #[arg(long)]
    pub cas_dir: Option<PathBuf>,
    /// Destination path for the pulled capsule manifest (defaults to <cas_dir>/capsule-manifest.json)
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Source registry (local OCI layout directory or remote reference)
    #[arg(long)]
    pub registry: String,
    /// OCI-style reference identifying the capsule to pull
    #[arg(long)]
    pub reference: String,
    /// Emit JSON summary
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone)]
struct PushLayer {
    digest: String,
    media_type: String,
    size: u64,
    annotations: HashMap<String, String>,
    source_path: PathBuf,
}

#[derive(Debug, Default)]
struct IngestReport {
    stored: usize,
    reused: usize,
}

#[derive(Debug)]
enum RegistryTarget {
    Local(PathBuf),
    Remote(RemoteDestination),
}

#[derive(Debug, Clone)]
struct RemoteDestination {
    scheme: String,
    base_url: String,
    host: String,
    repository: String,
    reference: RemoteReferenceKind,
    display: String,
    original_reference: String,
}

#[derive(Debug, Clone)]
enum RemoteReferenceKind {
    Tag(String),
    Digest(String),
}

impl RemoteReferenceKind {
    fn as_str(&self) -> &str {
        match self {
            RemoteReferenceKind::Tag(tag) => tag.as_str(),
            RemoteReferenceKind::Digest(digest) => digest.as_str(),
        }
    }
}

fn load_registry_auth_header() -> Result<Option<HeaderValue>> {
    if let Ok(raw) = env::var("ADEP_REGISTRY_AUTH_HEADER") {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let value = HeaderValue::from_str(trimmed)
            .context("ADEP_REGISTRY_AUTH_HEADER contains invalid header value")?;
        return Ok(Some(value));
    }

    if let Ok(token) = env::var("ADEP_REGISTRY_TOKEN") {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            let value = HeaderValue::from_str(&format!("Bearer {}", trimmed))
                .context("failed to construct Authorization header from ADEP_REGISTRY_TOKEN")?;
            return Ok(Some(value));
        }
    }

    let username = env::var("ADEP_REGISTRY_USERNAME").ok();
    let password = env::var("ADEP_REGISTRY_PASSWORD").ok();
    if let (Some(user), Some(pass)) = (username, password) {
        let credentials = format!("{}:{}", user, pass);
        let encoded = base64_encode(credentials);
        let value = HeaderValue::from_str(&format!("Basic {}", encoded))
            .context("failed to encode basic auth header for registry")?;
        return Ok(Some(value));
    }

    match default_deps_key_path() {
        Ok(path) => {
            if path.exists() {
                let stored = StoredKey::read(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?;
                let value = HeaderValue::from_str(&format!(
                    "AdepKey {}",
                    stored.developer_key_fingerprint()
                ))
                .context("failed to construct AdepKey authorization header")?;
                return Ok(Some(value));
            }
        }
        Err(_) => {
            // Ignore errors resolving default key path; proceed without header.
        }
    }

    Ok(None)
}

fn resolve_registry_target(root: &Path, registry: &str, reference: &str) -> Result<RegistryTarget> {
    if registry.contains("://") {
        let destination = parse_remote_destination(registry, reference)?;
        Ok(RegistryTarget::Remote(destination))
    } else {
        let registry_root = resolve_path(root, Path::new(registry));
        Ok(RegistryTarget::Local(registry_root))
    }
}

fn parse_remote_destination(registry: &str, reference: &str) -> Result<RemoteDestination> {
    let url = Url::parse(registry)
        .with_context(|| format!("failed to parse registry URL '{}'", registry))?;
    let scheme = url.scheme();
    let mut resolved_scheme = match scheme {
        "http" => "http",
        "https" => "https",
        "oci" => "https",
        other => bail!("unsupported registry scheme '{}'", other),
    };

    if scheme == "oci" && env::var("ADEP_REGISTRY_ALLOW_INSECURE").ok().as_deref() == Some("1") {
        resolved_scheme = "http";
    }

    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("registry URL '{}' must include a host", registry))?;
    let mut host_port = host.to_string();
    if let Some(port) = url.port() {
        host_port = format!("{}:{}", host_port, port);
    }

    let mut prefix = url.path().trim_matches('/').to_string();
    if prefix.ends_with('/') {
        prefix.pop();
    }
    if prefix.starts_with('/') {
        prefix.remove(0);
    }

    let (repo_part, reference_kind) = if let Some((repo, digest)) = reference.rsplit_once('@') {
        if repo.trim().is_empty() {
            bail!(
                "OCI reference '{}' must include a repository component before '@'",
                reference
            );
        }
        (
            repo.trim(),
            RemoteReferenceKind::Digest(digest.trim().to_string()),
        )
    } else if let Some((repo, tag)) = reference.rsplit_once(':') {
        if repo.trim().is_empty() {
            bail!(
                "OCI reference '{}' must include a repository component before ':'",
                reference
            );
        }
        (
            repo.trim(),
            RemoteReferenceKind::Tag(tag.trim().to_string()),
        )
    } else {
        bail!(
            "OCI reference '{}' must include a tag (:) or digest (@)",
            reference
        );
    };

    let repository = if prefix.is_empty() {
        repo_part.to_string()
    } else {
        format!("{}/{}", prefix, repo_part)
    };
    if repository.trim().is_empty() {
        bail!("resolved repository name must not be empty");
    }

    let base_url = format!("{}://{}", resolved_scheme, host_port);
    let display = format!("{}/{}", base_url, repository);

    Ok(RemoteDestination {
        scheme: resolved_scheme.to_string(),
        base_url,
        host: host_port,
        repository,
        reference: reference_kind,
        display,
        original_reference: reference.to_string(),
    })
}

impl RemoteDestination {
    fn reference_string(&self) -> &str {
        &self.original_reference
    }

    fn manifest_target(&self) -> &str {
        self.reference.as_str()
    }

    fn blob_url(&self, digest: &str) -> String {
        format!("{}/v2/{}/blobs/{}", self.base_url, self.repository, digest)
    }

    fn upload_url(&self, digest: &str) -> String {
        format!(
            "{}/v2/{}/blobs/uploads/?digest={}",
            self.base_url, self.repository, digest
        )
    }

    fn manifest_url(&self) -> String {
        format!(
            "{}/v2/{}/manifests/{}",
            self.base_url,
            self.repository,
            self.manifest_target()
        )
    }

    fn display(&self) -> &str {
        &self.display
    }
}

const CAPSULE_SCHEMA_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapsuleManifestPayload {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    #[serde(rename = "generatedAt")]
    generated_at: DateTime<Utc>,
    package: CapsulePackageInfo,
    generator: CapsuleGenerator,
    entries: Vec<IndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapsulePackageInfo {
    #[serde(rename = "id")]
    id: Uuid,
    #[serde(rename = "familyId")]
    family_id: Uuid,
    version: String,
    channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapsuleGenerator {
    tool: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapsuleSignature {
    algorithm: String,
    key: String,
    value: String,
    #[serde(rename = "payloadSha256")]
    payload_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapsuleManifest {
    #[serde(flatten)]
    data: CapsuleManifestPayload,
    signature: CapsuleSignature,
}

impl CapsuleManifestPayload {
    fn from_manifest(manifest: &Manifest, entries: Vec<IndexEntry>) -> Self {
        CapsuleManifestPayload {
            schema_version: CAPSULE_SCHEMA_VERSION.to_string(),
            generated_at: Utc::now(),
            package: CapsulePackageInfo {
                id: manifest.id,
                family_id: manifest.family_id,
                version: manifest.version.number.clone(),
                channel: manifest.version.channel.clone(),
                commit: manifest.version.commit.clone(),
                label: manifest.version.label.clone(),
            },
            generator: CapsuleGenerator {
                tool: "adep-cli".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            entries,
        }
    }
}

impl CapsuleManifest {
    fn sign(
        payload: CapsuleManifestPayload,
        keypair: &Keypair,
        key_fingerprint: &str,
    ) -> Result<Self> {
        let payload_bytes =
            serde_json::to_vec(&payload).context("failed to serialize capsule manifest payload")?;
        let payload_sha256 = hex::encode(Sha256::digest(&payload_bytes));
        let signature = keypair.sign(&payload_bytes);
        let signature_b64 = base64_encode(signature.to_bytes());
        let manifest = CapsuleManifest {
            data: payload,
            signature: CapsuleSignature {
                algorithm: "Ed25519".to_string(),
                key: key_fingerprint.to_string(),
                value: signature_b64,
                payload_sha256,
            },
        };
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<()> {
        if self.data.schema_version != CAPSULE_SCHEMA_VERSION {
            bail!(
                "unsupported capsule schemaVersion '{}' (expected {})",
                self.data.schema_version,
                CAPSULE_SCHEMA_VERSION
            );
        }
        if self.data.entries.is_empty() {
            bail!("capsule manifest must include at least one CAS entry");
        }
        let mut seen_paths = HashSet::new();
        for entry in &self.data.entries {
            if entry.path.trim().is_empty() {
                bail!("capsule entry path must not be empty");
            }
            if !seen_paths.insert(entry.path.clone()) {
                bail!("duplicate capsule entry path detected: {}", entry.path);
            }
        }
        if self.signature.algorithm.to_lowercase() != "ed25519" {
            bail!(
                "unsupported capsule signature algorithm '{}'",
                self.signature.algorithm
            );
        }
        let payload_bytes = serde_json::to_vec(&self.data)
            .context("failed to serialize capsule manifest payload for verification")?;
        let payload_sha = hex::encode(Sha256::digest(&payload_bytes));
        if payload_sha != self.signature.payload_sha256 {
            bail!(
                "capsule signature payload hash mismatch (expected {}, computed {})",
                self.signature.payload_sha256,
                payload_sha
            );
        }
        let signature_bytes = base64_decode(&self.signature.value)
            .context("failed to decode capsule signature value")?;
        if signature_bytes.len() != 64 {
            bail!(
                "capsule signature must be 64 bytes after decoding, got {}",
                signature_bytes.len()
            );
        }
        let signature =
            Signature::from_bytes(&signature_bytes).context("invalid Ed25519 signature bytes")?;
        let key_str = self
            .signature
            .key
            .strip_prefix("ed25519:")
            .ok_or_else(|| anyhow!("capsule signature key must start with 'ed25519:'"))?;
        let public_bytes =
            base64_decode(key_str).context("failed to decode signature public key")?;
        if public_bytes.len() != 32 {
            bail!(
                "capsule signature public key must be 32 bytes, got {}",
                public_bytes.len()
            );
        }
        let public = PublicKey::from_bytes(&public_bytes)
            .context("invalid Ed25519 public key provided in capsule signature")?;
        public
            .verify(&payload_bytes, &signature)
            .context("capsule signature verification failed")?;
        Ok(())
    }

    fn entries(&self) -> &[IndexEntry] {
        &self.data.entries
    }

    fn generated_at(&self) -> DateTime<Utc> {
        self.data.generated_at
    }

    fn package(&self) -> &CapsulePackageInfo {
        &self.data.package
    }

    fn signature(&self) -> &CapsuleSignature {
        &self.signature
    }
}

#[derive(Debug)]
struct OciLayout {
    root: PathBuf,
    index_path: PathBuf,
}

impl OciLayout {
    fn ensure(root: PathBuf) -> Result<Self> {
        if !root.exists() {
            fs::create_dir_all(&root)
                .with_context(|| format!("failed to create registry {}", root.display()))?;
        }
        let blobs_dir = root.join("blobs").join("sha256");
        fs::create_dir_all(&blobs_dir)
            .with_context(|| format!("failed to create blobs directory {}", blobs_dir.display()))?;
        let layout_file = root.join("oci-layout");
        if !layout_file.exists() {
            fs::write(&layout_file, r#"{"imageLayoutVersion":"1.0.0"}"#)
                .with_context(|| format!("failed to initialize {}", layout_file.display()))?;
        }
        let index_path = root.join("index.json");
        if !index_path.exists() {
            let index = OciIndex::default();
            let json = serde_json::to_vec_pretty(&index).expect("serialize oci index");
            fs::write(&index_path, json)
                .with_context(|| format!("failed to initialize {}", index_path.display()))?;
        }
        Ok(Self { root, index_path })
    }

    fn blob_path(&self, digest: &str) -> Result<PathBuf> {
        let (_, hex) = split_digest(digest)?;
        Ok(self.root.join("blobs").join("sha256").join(hex))
    }

    fn ensure_blob_from_bytes(&self, digest: &str, bytes: &[u8]) -> Result<bool> {
        let path = self.blob_path(digest)?;
        if path.exists() {
            return Ok(false);
        }
        fs::write(&path, bytes)
            .with_context(|| format!("failed to write blob {}", path.display()))?;
        Ok(true)
    }

    fn ensure_blob_from_path(&self, digest: &str, source: &Path) -> Result<bool> {
        let path = self.blob_path(digest)?;
        if path.exists() {
            return Ok(false);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::copy(source, &path)
            .with_context(|| format!("failed to copy blob to {}", path.display()))?;
        Ok(true)
    }

    fn read_blob(&self, digest: &str) -> Result<Vec<u8>> {
        let path = self.blob_path(digest)?;
        fs::read(&path)
            .with_context(|| format!("failed to read blob {} ({})", digest, path.display()))
    }

    fn load_index(&self) -> Result<OciIndex> {
        let raw = fs::read_to_string(&self.index_path)
            .with_context(|| format!("failed to read {}", self.index_path.display()))?;
        let index: OciIndex = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse {}", self.index_path.display()))?;
        Ok(index)
    }

    fn write_index(&self, index: &OciIndex) -> Result<()> {
        let json =
            serde_json::to_vec_pretty(index).context("failed to serialize OCI index manifest")?;
        fs::write(&self.index_path, json)
            .with_context(|| format!("failed to write {}", self.index_path.display()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OciIndex {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    manifests: Vec<OciIndexEntry>,
}

impl Default for OciIndex {
    fn default() -> Self {
        Self {
            schema_version: 2,
            manifests: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OciIndexEntry {
    #[serde(rename = "mediaType")]
    media_type: String,
    digest: String,
    size: u64,
    #[serde(default)]
    annotations: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OciDescriptor {
    #[serde(rename = "mediaType")]
    media_type: String,
    digest: String,
    size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    annotations: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OciImageManifest {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(rename = "mediaType")]
    media_type: String,
    config: OciDescriptor,
    layers: Vec<OciDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    annotations: Option<HashMap<String, String>>,
}

fn resolve_manifest_reference(index: &OciIndex, reference: &str) -> Result<String> {
    if let Some((_, digest)) = reference.split_once('@') {
        let normalized = normalize_digest(digest)?;
        if index.manifests.iter().any(|m| m.digest == normalized) {
            return Ok(normalized);
        } else {
            bail!(
                "manifest digest '{}' not found in registry index",
                normalized
            );
        }
    }
    for entry in &index.manifests {
        if entry
            .annotations
            .get("org.opencontainers.image.ref.name")
            .map(|value| value.as_str())
            == Some(reference)
        {
            return Ok(entry.digest.clone());
        }
    }
    bail!("reference '{}' not found in registry index", reference);
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn split_digest(digest: &str) -> Result<(&str, &str)> {
    if let Some(rest) = digest.strip_prefix("sha256:") {
        return Ok(("sha256", rest));
    }
    if let Some(rest) = digest.strip_prefix("sha256-") {
        return Ok(("sha256", rest));
    }
    digest
        .split_once(':')
        .or_else(|| digest.split_once('-'))
        .ok_or_else(|| anyhow!("unsupported digest format '{}'", digest))
}

fn normalize_digest(value: &str) -> Result<String> {
    let (_, hex) = split_digest(value)?;
    Ok(format!("sha256:{hex}"))
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum VendorLanguage {
    Python,
    Node,
}

#[derive(Debug, Clone)]
struct VendorOutcome {
    artifact: ArtifactMetadata,
    blob: StoredBlob,
    index_entry: IndexEntry,
}

#[derive(Debug, Clone)]
enum ArtifactMetadata {
    PythonWheel(WheelMetadata),
    PnpmTarball(PnpmMetadata),
}

impl ArtifactMetadata {
    fn to_json(&self) -> serde_json::Value {
        match self {
            ArtifactMetadata::PythonWheel(meta) => json!({
                "kind": "python_wheel",
                "distribution": meta.distribution,
                "version": meta.version,
                "python_tag": meta.python_tag,
                "abi_tag": meta.abi_tag,
                "platform_tags": meta.platform_tags,
                "build_tag": meta.build_tag,
                "metadata_name": meta.metadata_name,
                "metadata_version": meta.metadata_version,
                "summary": meta.summary,
                "requires_python": meta.requires_python,
                "requires_dist": meta.requires_dist,
            }),
            ArtifactMetadata::PnpmTarball(meta) => json!({
                "kind": "pnpm_tarball",
                "package": meta.package_name,
                "version": meta.version,
                "description": meta.description,
                "license": meta.license,
                "dependencies": meta.dependencies,
            }),
        }
    }

    fn describe(&self) -> String {
        match self {
            ArtifactMetadata::PythonWheel(meta) => match &meta.summary {
                Some(summary) if !summary.is_empty() => {
                    format!(
                        "Python wheel {} {} — {}",
                        meta.distribution, meta.version, summary
                    )
                }
                _ => format!("Python wheel {} {}", meta.distribution, meta.version),
            },
            ArtifactMetadata::PnpmTarball(meta) => match &meta.description {
                Some(desc) if !desc.is_empty() => {
                    format!(
                        "pnpm tarball {} {} — {}",
                        meta.package_name, meta.version, desc
                    )
                }
                _ => format!("pnpm tarball {} {}", meta.package_name, meta.version),
            },
        }
    }
}

#[derive(Debug, Clone)]
struct WheelMetadata {
    distribution: String,
    version: String,
    build_tag: Option<String>,
    python_tag: String,
    abi_tag: String,
    platform_tags: Vec<String>,
    metadata_name: String,
    metadata_version: Option<String>,
    summary: Option<String>,
    requires_python: Option<String>,
    requires_dist: Vec<String>,
}

#[derive(Debug, Clone)]
struct PnpmMetadata {
    package_name: String,
    version: String,
    description: Option<String>,
    license: Option<String>,
    dependencies: BTreeMap<String, String>,
}

#[derive(Debug)]
struct ParsedWheelMetadata {
    name: String,
    version: String,
    metadata_version: Option<String>,
    summary: Option<String>,
    requires_python: Option<String>,
    requires_dist: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PackageJson {
    name: String,
    version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
}

pub fn run(args: DepsArgs) -> Result<()> {
    match args.command {
        DepsCommand::Verify(cmd) => run_verify(&cmd),
        DepsCommand::Vendor(cmd) => run_vendor(&cmd),
        DepsCommand::Capsule(cmd) => run_capsule(&cmd),
        DepsCommand::Resolve(cmd) => run_resolve(&cmd),
        DepsCommand::Install(cmd) => run_install(&cmd),
        DepsCommand::Push(cmd) => run_push(&cmd),
        DepsCommand::Pull(cmd) => run_pull(&cmd),
    }
}

fn resolve_depsd_endpoint() -> Option<String> {
    match env::var("ADEP_DEPSD_ENDPOINT") {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

fn depsd_autostart_enabled() -> bool {
    match env::var("ADEP_DEPSD_AUTOSTART") {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            !(value == "0" || value == "false" || value == "off")
        }
        Err(_) => true,
    }
}

struct ManagedDepsd {
    endpoint: String,
    child: Child,
}

impl ManagedDepsd {
    fn start() -> Result<Self> {
        let binary = find_depsd_binary()?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .context("failed to bind ephemeral port for depsd")?;
        let addr = listener
            .local_addr()
            .context("failed to read ephemeral port")?;
        drop(listener);
        let endpoint = format!("127.0.0.1:{}", addr.port());
        let child = Command::new(&binary)
            .env("ADEP_DEPSD_LISTEN", &endpoint)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to spawn {}", binary.display()))?;
        Ok(Self { endpoint, child })
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn wait_until_ready(&mut self, runtime: &tokio::runtime::Runtime) -> Result<()> {
        if let Some(status) = self
            .child
            .try_wait()
            .context("failed to poll depsd status")?
        {
            bail!("depsd exited prematurely with status {}", status);
        }
        runtime.block_on(wait_for_depsd(&self.endpoint))
    }
}

impl Drop for ManagedDepsd {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn find_depsd_binary() -> Result<PathBuf> {
    if let Ok(path) = env::var("ADEP_DEPSD_BIN") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Ok(candidate);
        }
        bail!(
            "ADEP_DEPSD_BIN points to missing file: {}",
            candidate.display()
        );
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let candidate = dir.join(depsd_binary_name());
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    if let Some(path_env) = env::var_os("PATH") {
        for dir in env::split_paths(&path_env) {
            let candidate = dir.join(depsd_binary_name());
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    bail!("depsd binary not found; set ADEP_DEPSD_BIN or install depsd in PATH");
}

fn depsd_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "depsd.exe"
    }
    #[cfg(not(windows))]
    {
        "depsd"
    }
}

async fn wait_for_depsd(endpoint: &str) -> Result<()> {
    use tokio::time::{sleep, Duration as TokioDuration, Instant as TokioInstant};

    let timeout = TokioDuration::from_secs(10);
    let deadline = TokioInstant::now() + timeout;
    loop {
        match DepsdClient::connect(endpoint).await {
            Ok(mut client) => {
                if client.health_check().await.is_ok() {
                    return Ok(());
                }
            }
            Err(_) => {}
        }
        if TokioInstant::now() >= deadline {
            bail!("depsd at {} did not respond within {:?}", endpoint, timeout);
        }
        sleep(TokioDuration::from_millis(100)).await;
    }
}

fn run_capsule(args: &CapsuleArgs) -> Result<()> {
    let root = resolve_root(&args.root)?;
    let manifest_path = args
        .manifest
        .as_ref()
        .map(|path| resolve_path(&root, path))
        .unwrap_or_else(|| root.join("manifest.json"));
    if !manifest_path.exists() {
        bail!(
            "manifest {} does not exist; specify --manifest",
            manifest_path.display()
        );
    }
    let manifest = Manifest::load(&manifest_path)
        .with_context(|| format!("failed to load {}", manifest_path.display()))?;

    let index_path = match &args.index {
        Some(path) => resolve_path(&root, path),
        None => infer_default_path(&root, Some(&manifest), "cas/index.json")?,
    };
    let blobs_dir = match &args.blobs_dir {
        Some(path) => resolve_path(&root, path),
        None => infer_default_path(&root, Some(&manifest), "cas/blobs")?,
    };

    let output_path = match &args.output {
        Some(path) => resolve_path(&root, path),
        None => root.join("cas").join("capsule-manifest.json"),
    };
    let key_path = match &args.key {
        Some(path) => resolve_path(&root, path),
        None => default_deps_key_path()?,
    };

    let stored_key = StoredKey::read(&key_path)
        .with_context(|| format!("failed to read key file {}", key_path.display()))?;
    let keypair = stored_key
        .to_keypair()
        .context("failed to construct Ed25519 keypair from key file")?;
    let file = File::open(&index_path)
        .with_context(|| format!("failed to open index {}", index_path.display()))?;
    let index = CanonicalIndex::from_reader(file)?;
    if index.is_empty() {
        bail!(
            "canonical index {} is empty; nothing to include in capsule",
            index_path.display()
        );
    }

    let mut entries = index.entries().to_vec();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    for entry in &entries {
        package::ensure_safe_relative_path(Path::new(&entry.path))
            .with_context(|| format!("invalid entry path '{}'", entry.path))?;
        let blob_path = blobs_dir.join(&entry.path);
        if !blob_path.exists() {
            bail!(
                "CAS blob '{0}' not found at {1}; run 'adep deps vendor' or specify --blobs-dir",
                entry.path,
                blob_path.display()
            );
        }
    }

    let payload = CapsuleManifestPayload::from_manifest(&manifest, entries);
    let capsule =
        CapsuleManifest::sign(payload, &keypair, &stored_key.developer_key_fingerprint())?;
    capsule.validate()?;

    write_json_atomically(&output_path, &capsule)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    let entry_count = capsule.entries().len();
    let signature = capsule.signature().clone();
    let package_info = capsule.package().clone();
    let summary = json!({
        "manifest": output_path.display().to_string(),
        "entry_count": entry_count,
        "generated_at": capsule.generated_at().to_rfc3339(),
        "signature": {
            "key": signature.key,
            "payload_sha256": signature.payload_sha256,
        },
        "package": {
            "id": package_info.id,
            "family_id": package_info.family_id,
            "version": package_info.version,
            "channel": package_info.channel,
            "commit": package_info.commit,
            "label": package_info.label,
        },
        "cas": {
            "index": index_path.display().to_string(),
            "blobs": blobs_dir.display().to_string(),
        },
        "key_path": key_path.display().to_string(),
    });

    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        let entry_label = if entry_count == 1 { "entry" } else { "entries" };
        println!(
            "Generated capsule manifest with {} {} -> {}",
            entry_count,
            entry_label,
            output_path.display()
        );
        println!(
            "  signature key: {} | payload sha256: {}",
            signature.key, signature.payload_sha256
        );
        println!(
            "  index: {} | blobs: {} | key: {}",
            index_path.display(),
            blobs_dir.display(),
            key_path.display()
        );
    }
    Ok(())
}

fn run_resolve(args: &ResolveArgs) -> Result<()> {
    let root = resolve_root(&args.root)?;
    let manifest_path = args
        .manifest
        .as_ref()
        .map(|path| resolve_path(&root, path))
        .unwrap_or_else(|| root.join("manifest.json"));
    if !manifest_path.exists() {
        bail!(
            "manifest {} does not exist; specify --manifest",
            manifest_path.display()
        );
    }
    let manifest = Manifest::load(&manifest_path)
        .with_context(|| format!("failed to load {}", manifest_path.display()))?;
    let ctx = prepare_dependency_context(
        root,
        manifest,
        args.capsule.as_ref(),
        args.cas_dir.as_ref(),
        args.output.as_ref(),
    )?;
    let store = BlobStore::open(&ctx.cas_root)
        .with_context(|| format!("failed to open CAS root {}", ctx.cas_root.display()))?;
    let resolved = perform_resolve(
        &ctx.manifest,
        &ctx.capsule,
        &ctx.capsule_path,
        store.root(),
        &ctx.output_dir,
    )?;
    output_resolve_summary(&ctx, &resolved, args.json)?;
    Ok(())
}

fn run_install(args: &InstallArgs) -> Result<()> {
    let root = resolve_root(&args.root)?;
    let manifest_path = args
        .manifest
        .as_ref()
        .map(|path| resolve_path(&root, path))
        .unwrap_or_else(|| root.join("manifest.json"));
    if !manifest_path.exists() {
        bail!(
            "manifest {} does not exist; specify --manifest",
            manifest_path.display()
        );
    }
    let manifest = Manifest::load(&manifest_path)
        .with_context(|| format!("failed to load {}", manifest_path.display()))?;
    let mut depsd_endpoint = resolve_depsd_endpoint();
    let use_depsd = depsd_endpoint.is_some() || depsd_autostart_enabled();
    let ctx = prepare_dependency_context(
        root,
        manifest,
        args.capsule.as_ref(),
        args.cas_dir.as_ref(),
        args.output.as_ref(),
    )?;
    let store = BlobStore::open(&ctx.cas_root)
        .with_context(|| format!("failed to open CAS root {}", ctx.cas_root.display()))?;
    let resolved = perform_resolve(
        &ctx.manifest,
        &ctx.capsule,
        &ctx.capsule_path,
        store.root(),
        &ctx.output_dir,
    )?;

    let mut python_result = None;
    let mut node_result = None;
    let mut python_request: Option<InstallPythonRequest> = None;
    let mut pnpm_request: Option<InstallPnpmRequest> = None;
    if let Some(deps) = ctx.manifest.deps.as_ref() {
        if let Some(py_cfg) = deps.python.as_ref() {
            let artifacts = resolved.python.as_ref().ok_or_else(|| {
                anyhow!("capsule does not include python-wheel entries required by deps.python")
            })?;
            if use_depsd {
                let requirements_path = resolve_path(&ctx.root, Path::new(&py_cfg.requirements));
                if !requirements_path.exists() {
                    bail!(
                        "requirements file {} does not exist",
                        requirements_path.display()
                    );
                }
                let pip_bin_path = args
                    .pip
                    .as_ref()
                    .map(|path| resolve_path(&ctx.root, path))
                    .unwrap_or_else(|| PathBuf::from("pip"));
                let mut pip_args = Vec::new();
                if let Some(install) = &py_cfg.install {
                    if let Some(target) = &install.target {
                        let target_path = resolve_path(&ctx.root, Path::new(target));
                        if !args.dry_run {
                            fs::create_dir_all(&target_path).with_context(|| {
                                format!(
                                    "failed to create pip target directory {}",
                                    target_path.display()
                                )
                            })?;
                        }
                        pip_args.push("--target".to_string());
                        pip_args.push(target_path.display().to_string());
                    }
                }
                python_request = Some(InstallPythonRequest {
                    capsule_path: ctx.capsule_path.display().to_string(),
                    cas_root: ctx.cas_root.display().to_string(),
                    requirements_lock: requirements_path.display().to_string(),
                    target_dir: artifacts.wheels_dir.display().to_string(),
                    pip_binary: pip_bin_path.display().to_string(),
                    pip_args,
                    dry_run: args.dry_run,
                    project_dir: ctx.root.display().to_string(),
                });
            } else {
                python_result = Some(run_pip_install(
                    &ctx,
                    py_cfg,
                    artifacts,
                    args.pip.as_ref(),
                    args.dry_run,
                )?);
            }
        }
        if let Some(node_cfg) = deps.node.as_ref() {
            let artifacts = resolved.node.as_ref().ok_or_else(|| {
                anyhow!("capsule does not include pnpm tarball entries required by deps.node")
            })?;
            if use_depsd {
                let lockfile_path = resolve_path(&ctx.root, Path::new(&node_cfg.lockfile));
                if !lockfile_path.exists() {
                    bail!("lockfile {} does not exist", lockfile_path.display());
                }
                let pnpm_bin_path = args
                    .pnpm
                    .as_ref()
                    .map(|path| resolve_path(&ctx.root, path))
                    .unwrap_or_else(|| PathBuf::from("pnpm"));
                let mut pnpm_args = Vec::new();
                if let Some(install) = &node_cfg.install {
                    if install.frozen_lockfile.map(|value| !value).unwrap_or(false) {
                        pnpm_args.push("--no-frozen-lockfile".to_string());
                    }
                }
                pnpm_request = Some(InstallPnpmRequest {
                    capsule_path: ctx.capsule_path.display().to_string(),
                    cas_root: ctx.cas_root.display().to_string(),
                    lockfile: lockfile_path.display().to_string(),
                    project_dir: ctx.root.display().to_string(),
                    store_dir: artifacts.store_dir.display().to_string(),
                    pnpm_binary: pnpm_bin_path.display().to_string(),
                    pnpm_args,
                    dry_run: args.dry_run,
                });
            } else {
                node_result = Some(run_pnpm_install(
                    &ctx,
                    node_cfg,
                    artifacts,
                    args.pnpm.as_ref(),
                    args.dry_run,
                )?);
            }
        }
    }

    if use_depsd && (python_request.is_some() || pnpm_request.is_some()) {
        let mut managed = None;
        let endpoint = match depsd_endpoint.take() {
            Some(endpoint) => endpoint,
            None => {
                let instance = ManagedDepsd::start().context("failed to launch depsd")?;
                let endpoint = instance.endpoint().to_string();
                managed = Some(instance);
                endpoint
            }
        };
        let runtime = tokio::runtime::Runtime::new()
            .context("failed to initialize tokio runtime for depsd install")?;
        if let Some(instance) = managed.as_mut() {
            instance
                .wait_until_ready(&runtime)
                .context("depsd did not become ready in time")?;
        }
        let python_req = python_request.take();
        let pnpm_req = pnpm_request.take();
        let (python_resp, pnpm_resp) = runtime.block_on(async move {
            let mut client = DepsdClient::connect(endpoint.clone())
                .await
                .map_err(|err| anyhow!("failed to connect to depsd at {}: {}", endpoint, err))?;
            client
                .health_check()
                .await
                .map_err(|status| map_depsd_status("health check", status))?;
            let mut python_response = None;
            if let Some(request) = python_req {
                let response = client
                    .install_python(request)
                    .await
                    .map_err(|status| map_depsd_status("python install", status))?;
                python_response = Some(response);
            }
            let mut pnpm_response = None;
            if let Some(request) = pnpm_req {
                let response = client
                    .install_pnpm(request)
                    .await
                    .map_err(|status| map_depsd_status("pnpm install", status))?;
                pnpm_response = Some(response);
            }
            Ok::<_, anyhow::Error>((python_response, pnpm_response))
        })?;

        if let Some(response) = python_resp {
            emit_depsd_logs(&response.logs);
            python_result = Some(InstallCommandResult {
                command: join_command_parts(&response.command),
                dry_run: args.dry_run,
            });
        }
        if let Some(response) = pnpm_resp {
            emit_depsd_logs(&response.logs);
            node_result = Some(InstallCommandResult {
                command: join_command_parts(&response.command),
                dry_run: args.dry_run,
            });
        }
        drop(managed);
    }

    output_install_summary(
        &ctx,
        &resolved,
        python_result.as_ref(),
        node_result.as_ref(),
        args.json,
    )?;
    Ok(())
}

#[derive(Debug)]
struct DependencyContext {
    root: PathBuf,
    manifest: Manifest,
    capsule_path: PathBuf,
    capsule: CapsuleManifest,
    cas_root: PathBuf,
    output_dir: PathBuf,
}

#[derive(Debug)]
struct ResolveOutput {
    python_requested: bool,
    python: Option<ResolvedPython>,
    node_requested: bool,
    node: Option<ResolvedNode>,
}

#[derive(Debug)]
struct ResolvedPython {
    wheels_dir: PathBuf,
    stored: usize,
    reused: usize,
    files: Vec<PathBuf>,
}

#[derive(Debug)]
struct ResolvedNode {
    store_dir: PathBuf,
    stored: usize,
    reused: usize,
    files: Vec<PathBuf>,
}

#[derive(Debug)]
struct InstallCommandResult {
    command: String,
    dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaterializeStatus {
    Stored,
    Reused,
}

#[derive(Debug)]
struct MaterializeOutcome {
    status: MaterializeStatus,
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CapsuleEntryKind {
    PythonWheel,
    PnpmTarball,
    Other,
}

fn prepare_dependency_context(
    root: PathBuf,
    manifest: Manifest,
    capsule_arg: Option<&PathBuf>,
    cas_arg: Option<&PathBuf>,
    output_arg: Option<&PathBuf>,
) -> Result<DependencyContext> {
    let capsule_path = capsule_arg
        .map(|path| resolve_path(&root, path))
        .unwrap_or_else(|| root.join("cas").join("capsule-manifest.json"));
    if !capsule_path.exists() {
        bail!(
            "capsule manifest {} does not exist; run 'adep deps capsule' or specify --capsule",
            capsule_path.display()
        );
    }
    let capsule = load_capsule_manifest(&capsule_path)?;
    let inferred_cas = capsule_path.parent().map(Path::to_path_buf);
    let cas_root = match cas_arg {
        Some(dir) => resolve_path(&root, dir),
        None => inferred_cas.unwrap_or(default_cas_root()?),
    };
    let output_dir = output_arg
        .map(|path| resolve_path(&root, path))
        .unwrap_or_else(|| root.join("deps-cache"));
    Ok(DependencyContext {
        root,
        manifest,
        capsule_path,
        capsule,
        cas_root,
        output_dir,
    })
}

fn perform_resolve(
    manifest: &Manifest,
    capsule: &CapsuleManifest,
    capsule_path: &Path,
    cas_root: &Path,
    output_dir: &Path,
) -> Result<ResolveOutput> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let python_requested = manifest
        .deps
        .as_ref()
        .and_then(|deps| deps.python.as_ref())
        .is_some();
    let node_requested = manifest
        .deps
        .as_ref()
        .and_then(|deps| deps.node.as_ref())
        .is_some();

    let mut python_entries: Vec<&IndexEntry> = Vec::new();
    let mut node_entries: Vec<&IndexEntry> = Vec::new();
    for entry in capsule.entries() {
        match detect_entry_kind(entry) {
            CapsuleEntryKind::PythonWheel => python_entries.push(entry),
            CapsuleEntryKind::PnpmTarball => node_entries.push(entry),
            CapsuleEntryKind::Other => {}
        }
    }

    let mut python_result = None;
    if python_requested {
        if python_entries.is_empty() {
            bail!(
                "capsule manifest {} does not contain python-wheel entries required by deps.python",
                capsule_path.display()
            );
        }
        let wheels_dir = output_dir.join("python").join("wheels");
        fs::create_dir_all(&wheels_dir)
            .with_context(|| format!("failed to create {}", wheels_dir.display()))?;
        let mut stored = 0usize;
        let mut reused = 0usize;
        let mut files = Vec::new();
        for entry in python_entries {
            let file_name = entry
                .metadata
                .as_ref()
                .and_then(|m| m.filename.as_ref())
                .ok_or_else(|| {
                    anyhow!(
                        "capsule entry '{}' is missing metadata.filename for python wheel",
                        entry.path
                    )
                })?;
            let outcome = materialize_artifact(cas_root, entry, &wheels_dir, file_name)?;
            match outcome.status {
                MaterializeStatus::Stored => stored += 1,
                MaterializeStatus::Reused => reused += 1,
            }
            files.push(outcome.path);
        }
        python_result = Some(ResolvedPython {
            wheels_dir,
            stored,
            reused,
            files,
        });
    }

    let mut node_result = None;
    if node_requested {
        if node_entries.is_empty() {
            bail!(
                "capsule manifest {} does not contain pnpm tarball entries required by deps.node",
                capsule_path.display()
            );
        }
        let store_dir = output_dir.join("node").join("store");
        fs::create_dir_all(&store_dir)
            .with_context(|| format!("failed to create {}", store_dir.display()))?;
        let mut stored = 0usize;
        let mut reused = 0usize;
        let mut files = Vec::new();
        for entry in node_entries {
            let file_name = entry
                .metadata
                .as_ref()
                .and_then(|m| m.filename.as_ref())
                .ok_or_else(|| {
                    anyhow!(
                        "capsule entry '{}' is missing metadata.filename for pnpm tarball",
                        entry.path
                    )
                })?;
            let outcome = materialize_artifact(cas_root, entry, &store_dir, file_name)?;
            match outcome.status {
                MaterializeStatus::Stored => stored += 1,
                MaterializeStatus::Reused => reused += 1,
            }
            files.push(outcome.path);
        }
        node_result = Some(ResolvedNode {
            store_dir,
            stored,
            reused,
            files,
        });
    }

    Ok(ResolveOutput {
        python_requested,
        python: python_result,
        node_requested,
        node: node_result,
    })
}

fn detect_entry_kind(entry: &IndexEntry) -> CapsuleEntryKind {
    if let Some(meta) = &entry.metadata {
        if let Some(kind) = meta.kind.as_deref() {
            match kind {
                "python-wheel" => return CapsuleEntryKind::PythonWheel,
                "pnpm-tarball" => return CapsuleEntryKind::PnpmTarball,
                _ => {}
            }
        }
    }
    for coord in &entry.coords {
        if coord.starts_with("pkg:pypi/") {
            return CapsuleEntryKind::PythonWheel;
        }
        if coord.starts_with("pkg:npm/") {
            return CapsuleEntryKind::PnpmTarball;
        }
    }
    CapsuleEntryKind::Other
}

fn materialize_artifact(
    cas_root: &Path,
    entry: &IndexEntry,
    dest_dir: &Path,
    file_name: &str,
) -> Result<MaterializeOutcome> {
    package::ensure_safe_relative_path(Path::new(file_name))
        .with_context(|| format!("invalid artifact filename '{}'", file_name))?;
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("failed to create {}", dest_dir.display()))?;
    let dest_path = dest_dir.join(file_name);
    if dest_path.exists() {
        let existing_sha = package::hash_file_hex(&dest_path)
            .with_context(|| format!("failed to hash {}", dest_path.display()))?;
        if existing_sha == entry.raw_sha256 {
            return Ok(MaterializeOutcome {
                status: MaterializeStatus::Reused,
                path: dest_path,
            });
        }
        fs::remove_file(&dest_path).with_context(|| {
            format!("failed to remove outdated artifact {}", dest_path.display())
        })?;
    }

    package::ensure_safe_relative_path(Path::new(&entry.path))
        .with_context(|| format!("invalid capsule entry path '{}'", entry.path))?;
    let source_path = cas_root.join("blobs").join(&entry.path);
    if !source_path.exists() {
        bail!(
            "CAS blob '{}' missing at {}; run 'adep deps pull' first",
            entry.path,
            source_path.display()
        );
    }
    let file = File::open(&source_path)
        .with_context(|| format!("failed to open blob {}", source_path.display()))?;
    let mut reader: Box<dyn Read> = match entry.compressed.as_ref() {
        Some(compressed) => match compressed.alg.as_str() {
            "zstd" => Box::new(
                ZstdDecoder::new(BufReader::new(file))
                    .with_context(|| format!("failed to decompress {}", source_path.display()))?,
            ),
            other => {
                bail!(
                    "unsupported compression algorithm '{}' for entry '{}'",
                    other,
                    entry.path
                )
            }
        },
        None => Box::new(BufReader::new(file)),
    };
    let mut temp = NamedTempFile::new_in(dest_dir)
        .with_context(|| format!("failed to create temp file in {}", dest_dir.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .with_context(|| format!("failed to read blob {}", entry.path))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        temp.write_all(&buffer[..read])
            .with_context(|| format!("failed to write temp artifact {}", file_name))?;
    }
    temp.flush()
        .with_context(|| format!("failed to flush temp artifact {}", file_name))?;
    let digest = hex::encode(hasher.finalize());
    if digest != entry.raw_sha256 {
        bail!(
            "raw hash mismatch for '{}' (expected {}, computed {})",
            entry.path,
            entry.raw_sha256,
            digest
        );
    }
    temp.persist(&dest_path).map_err(|err| {
        anyhow!(
            "failed to persist artifact {}: {}",
            dest_path.display(),
            err.error
        )
    })?;
    Ok(MaterializeOutcome {
        status: MaterializeStatus::Stored,
        path: dest_path,
    })
}

fn run_pip_install(
    ctx: &DependencyContext,
    config: &PythonDependencies,
    artifacts: &ResolvedPython,
    pip_override: Option<&PathBuf>,
    dry_run: bool,
) -> Result<InstallCommandResult> {
    let pip_bin = pip_override
        .map(|path| resolve_path(&ctx.root, path))
        .unwrap_or_else(|| PathBuf::from("pip"));
    let requirements_path = resolve_path(&ctx.root, Path::new(&config.requirements));
    if !requirements_path.exists() {
        bail!(
            "requirements file {} does not exist",
            requirements_path.display()
        );
    }
    let mut cmd = Command::new(&pip_bin);
    cmd.arg("install")
        .arg("--require-hashes")
        .arg("--no-deps")
        .arg("--no-index")
        .arg("--no-input")
        .arg("--disable-pip-version-check")
        .arg("--find-links")
        .arg(&artifacts.wheels_dir)
        .arg("-r")
        .arg(&requirements_path);
    if let Some(install) = &config.install {
        if let Some(target) = &install.target {
            let target_path = resolve_path(&ctx.root, Path::new(target));
            if !dry_run {
                fs::create_dir_all(&target_path).with_context(|| {
                    format!(
                        "failed to create pip target directory {}",
                        target_path.display()
                    )
                })?;
            }
            cmd.arg("--target");
            cmd.arg(&target_path);
        }
    }
    cmd.current_dir(&ctx.root);
    cmd.env("PIP_NO_INDEX", "1");
    cmd.env("PIP_DISABLE_PIP_VERSION_CHECK", "1");
    cmd.env("PIP_RETRIES", "0");

    let command_str = format_command(&cmd);
    if dry_run {
        return Ok(InstallCommandResult {
            command: command_str,
            dry_run: true,
        });
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to execute '{}'", command_str))?;
    if !status.success() {
        let code = status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "terminated by signal".to_string());
        bail!("pip install failed with status {} ({})", code, command_str);
    }
    Ok(InstallCommandResult {
        command: command_str,
        dry_run: false,
    })
}

fn run_pnpm_install(
    ctx: &DependencyContext,
    config: &NodeDependencies,
    artifacts: &ResolvedNode,
    pnpm_override: Option<&PathBuf>,
    dry_run: bool,
) -> Result<InstallCommandResult> {
    let pnpm_bin = pnpm_override
        .map(|path| resolve_path(&ctx.root, path))
        .unwrap_or_else(|| PathBuf::from("pnpm"));
    let lockfile_path = resolve_path(&ctx.root, Path::new(&config.lockfile));
    if !lockfile_path.exists() {
        bail!("lockfile {} does not exist", lockfile_path.display());
    }
    let mut cmd = Command::new(&pnpm_bin);
    cmd.arg("install")
        .arg("--offline")
        .arg("--frozen-lockfile")
        .arg("--store-dir")
        .arg(&artifacts.store_dir);
    cmd.current_dir(&ctx.root);
    cmd.env("PNPM_FETCH_RETRIES", "0");
    cmd.env("PNPM_NETWORK_CONCURRENCY", "1");

    let command_str = format_command(&cmd);
    if dry_run {
        return Ok(InstallCommandResult {
            command: command_str,
            dry_run: true,
        });
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to execute '{}'", command_str))?;
    if !status.success() {
        let code = status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "terminated by signal".to_string());
        bail!("pnpm install failed with status {} ({})", code, command_str);
    }
    Ok(InstallCommandResult {
        command: command_str,
        dry_run: false,
    })
}

fn build_resolve_json(ctx: &DependencyContext, resolved: &ResolveOutput) -> serde_json::Value {
    let python_value = if resolved.python_requested {
        resolved
            .python
            .as_ref()
            .map(|py| {
                json!({
                    "wheels_dir": py.wheels_dir.display().to_string(),
                    "stored": py.stored,
                    "reused": py.reused,
                    "files": py
                        .files
                        .iter()
                        .map(|f| f.display().to_string())
                        .collect::<Vec<_>>(),
                })
            })
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    let node_value = if resolved.node_requested {
        resolved
            .node
            .as_ref()
            .map(|node| {
                json!({
                    "store_dir": node.store_dir.display().to_string(),
                    "stored": node.stored,
                    "reused": node.reused,
                    "files": node
                        .files
                        .iter()
                        .map(|f| f.display().to_string())
                        .collect::<Vec<_>>(),
                })
            })
            .unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    json!({
        "output_dir": ctx.output_dir.display().to_string(),
        "cas_root": ctx.cas_root.display().to_string(),
        "capsule": ctx.capsule_path.display().to_string(),
        "python_requested": resolved.python_requested,
        "python": python_value,
        "node_requested": resolved.node_requested,
        "node": node_value,
    })
}

fn output_resolve_summary(
    ctx: &DependencyContext,
    resolved: &ResolveOutput,
    json: bool,
) -> Result<()> {
    if json {
        let summary = build_resolve_json(ctx, resolved);
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    println!(
        "Resolved dependency capsule {} into {}",
        ctx.capsule_path.display(),
        ctx.output_dir.display()
    );
    if resolved.python_requested {
        if let Some(py) = &resolved.python {
            println!(
                "  python wheels: stored {} | reused {} | dir {}",
                py.stored,
                py.reused,
                py.wheels_dir.display()
            );
        } else {
            println!("  python wheels: missing (capsule incomplete)");
        }
    } else {
        println!("  python wheels: skipped (manifest has no deps.python)");
    }
    if resolved.node_requested {
        if let Some(node) = &resolved.node {
            println!(
                "  pnpm store: stored {} | reused {} | dir {}",
                node.stored,
                node.reused,
                node.store_dir.display()
            );
        } else {
            println!("  pnpm store: missing (capsule incomplete)");
        }
    } else {
        println!("  pnpm store: skipped (manifest has no deps.node)");
    }
    Ok(())
}

fn output_install_summary(
    ctx: &DependencyContext,
    resolved: &ResolveOutput,
    python_result: Option<&InstallCommandResult>,
    node_result: Option<&InstallCommandResult>,
    json: bool,
) -> Result<()> {
    if json {
        let summary = json!({
            "resolve": build_resolve_json(ctx, resolved),
            "install": {
                "python": python_result
                    .map(|r| json!({
                        "command": r.command,
                        "dry_run": r.dry_run,
                    }))
                    .unwrap_or(serde_json::Value::Null),
                "node": node_result
                    .map(|r| json!({
                        "command": r.command,
                        "dry_run": r.dry_run,
                    }))
                    .unwrap_or(serde_json::Value::Null),
            },
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    output_resolve_summary(ctx, resolved, false)?;
    if let Some(result) = python_result {
        println!(
            "python install command: {}{}",
            result.command,
            if result.dry_run { " [dry-run]" } else { "" }
        );
    } else if resolved.python_requested {
        println!("python install command: skipped (capsule missing artifacts)");
    } else {
        println!("python install command: skipped (not configured)");
    }
    if let Some(result) = node_result {
        println!(
            "pnpm install command: {}{}",
            result.command,
            if result.dry_run { " [dry-run]" } else { "" }
        );
    } else if resolved.node_requested {
        println!("pnpm install command: skipped (capsule missing artifacts)");
    } else {
        println!("pnpm install command: skipped (not configured)");
    }
    Ok(())
}

fn format_command(cmd: &Command) -> String {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = cmd
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if args.is_empty() {
        program
    } else {
        format!("{} {}", program, args.join(" "))
    }
}

fn join_command_parts(parts: &[String]) -> String {
    if parts.is_empty() {
        String::new()
    } else {
        parts.join(" ")
    }
}

fn map_depsd_status(context: &str, status: Status) -> anyhow::Error {
    if !status.details().is_empty() {
        let mut cursor = Cursor::new(status.details());
        if let Ok(detail) = OperationError::decode(&mut cursor) {
            return anyhow!(
                "depsd {} failed [{}]: {}",
                context,
                detail.code,
                detail.message
            );
        }
    }
    anyhow!(
        "depsd {} failed (code {}): {}",
        context,
        status.code(),
        status.message()
    )
}

fn emit_depsd_logs(logs: &[CommandLog]) {
    for log in logs {
        let Some(stream) = command_log::Stream::try_from(log.stream).ok() else {
            continue;
        };
        match stream {
            command_log::Stream::Stdout => println!("{}", log.line),
            command_log::Stream::Stderr => eprintln!("{}", log.line),
        }
    }
}

fn run_push(args: &PushArgs) -> Result<()> {
    let root = resolve_root(&args.root)?;
    let capsule_path = args
        .capsule
        .as_ref()
        .map(|path| resolve_path(&root, path))
        .unwrap_or_else(|| root.join("cas").join("capsule-manifest.json"));
    if !capsule_path.exists() {
        bail!(
            "capsule manifest {} does not exist; run 'adep deps capsule' first or specify --capsule",
            capsule_path.display()
        );
    }

    let capsule = load_capsule_manifest(&capsule_path)?;
    let inferred_cas = capsule_path.parent().map(Path::to_path_buf);
    let cas_root = match &args.cas_dir {
        Some(dir) => resolve_path(&root, dir),
        None => inferred_cas.unwrap_or(default_cas_root()?),
    };
    let blobs_dir = cas_root.join("blobs");
    if !blobs_dir.exists() {
        bail!(
            "CAS blobs directory {} does not exist; specify --cas-dir",
            blobs_dir.display()
        );
    }

    let package_info = capsule.package().clone();
    let config_json = json!({
        "mediaType": "application/vnd.adep.capsule.config.v1+json",
        "generatedAt": capsule.generated_at().to_rfc3339(),
        "package": {
            "id": package_info.id,
            "family_id": package_info.family_id,
            "version": package_info.version,
            "channel": package_info.channel,
            "commit": package_info.commit,
            "label": package_info.label,
        }
    });
    let config_bytes = serde_json::to_vec(&config_json)?;
    let config_digest_hex = sha256_hex(&config_bytes);
    let config_digest = format!("sha256:{config_digest_hex}");

    let capsule_bytes = serde_json::to_vec(&capsule)?;
    let capsule_digest_hex = sha256_hex(&capsule_bytes);
    let capsule_digest = format!("sha256:{capsule_digest_hex}");

    let mut push_layers = Vec::new();
    for entry in capsule.entries() {
        package::ensure_safe_relative_path(Path::new(&entry.path))
            .with_context(|| format!("invalid entry path '{}'", entry.path))?;
        let compressed = entry
            .compressed_sha256
            .as_ref()
            .ok_or_else(|| anyhow!("capsule entry '{}' missing compressed_sha256", entry.path))?;
        let digest = format!("sha256:{compressed}");
        let source_blob = blobs_dir.join(&entry.path);
        if !source_blob.exists() {
            bail!(
                "CAS blob '{}' missing at {}; specify --cas-dir to point at the correct CAS root",
                entry.path,
                source_blob.display()
            );
        }
        let mut annotations = HashMap::new();
        annotations.insert("org.opencontainers.image.title".into(), entry.path.clone());
        if let Some(coord) = entry.coords.get(0) {
            annotations.insert("com.adep.cas.coord".into(), coord.clone());
        }
        if let Some(platform) = entry.platform.get(0) {
            annotations.insert("com.adep.cas.platform".into(), platform.clone());
        }
        let media_type = match entry.compressed.as_ref().map(|c| c.alg.as_str()) {
            Some("zstd") | None => "application/vnd.adep.cas.blob.v1+zstd".into(),
            Some(other) => format!("application/vnd.adep.cas.blob.v1+{}", other),
        };
        let blob_size = fs::metadata(&source_blob)
            .with_context(|| format!("failed to stat {}", source_blob.display()))?
            .len();
        push_layers.push(PushLayer {
            digest,
            media_type,
            size: blob_size,
            annotations,
            source_path: source_blob,
        });
    }

    let mut config_annotations = HashMap::new();
    config_annotations.insert(
        "org.opencontainers.image.title".into(),
        "capsule-config.json".into(),
    );
    let config_descriptor = OciDescriptor {
        media_type: "application/vnd.adep.capsule.config.v1+json".into(),
        digest: config_digest.clone(),
        size: config_bytes.len() as u64,
        annotations: Some(config_annotations),
    };

    let mut capsule_annotations = HashMap::new();
    capsule_annotations.insert(
        "org.opencontainers.image.title".into(),
        "capsule-manifest.json".into(),
    );
    capsule_annotations.insert(
        "com.adep.capsule.signature".into(),
        capsule.signature().value.clone(),
    );

    let mut manifest_layers = Vec::with_capacity(1 + push_layers.len());
    manifest_layers.push(OciDescriptor {
        media_type: "application/vnd.adep.capsule.manifest.v1+json".into(),
        digest: capsule_digest.clone(),
        size: capsule_bytes.len() as u64,
        annotations: Some(capsule_annotations),
    });
    for layer in &push_layers {
        manifest_layers.push(OciDescriptor {
            media_type: layer.media_type.clone(),
            digest: layer.digest.clone(),
            size: layer.size,
            annotations: Some(layer.annotations.clone()),
        });
    }

    let mut manifest_annotations = HashMap::new();
    manifest_annotations.insert(
        "org.opencontainers.image.created".into(),
        capsule.generated_at().to_rfc3339(),
    );
    manifest_annotations.insert(
        "org.opencontainers.artifact.type".into(),
        "application/vnd.adep.capsule.v1".into(),
    );
    manifest_annotations.insert("com.adep.package.id".into(), package_info.id.to_string());

    let manifest = OciImageManifest {
        schema_version: 2,
        media_type: "application/vnd.oci.image.manifest.v1+json".into(),
        config: config_descriptor,
        layers: manifest_layers,
        annotations: Some(manifest_annotations),
    };
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let manifest_digest_hex = sha256_hex(&manifest_bytes);
    let manifest_digest = format!("sha256:{manifest_digest_hex}");

    let registry_target = resolve_registry_target(&root, &args.registry, &args.reference)?;
    match registry_target {
        RegistryTarget::Local(registry_root) => push_to_local(
            registry_root,
            &args.reference,
            &capsule,
            &capsule_bytes,
            &capsule_digest,
            &config_bytes,
            &config_digest,
            &manifest,
            &manifest_bytes,
            &manifest_digest,
            &push_layers,
            args.json,
        ),
        RegistryTarget::Remote(destination) => push_to_remote(
            destination,
            &capsule,
            &capsule_bytes,
            &capsule_digest,
            &config_bytes,
            &config_digest,
            &manifest,
            &manifest_bytes,
            &manifest_digest,
            &push_layers,
            args.json,
        ),
    }
}

fn push_to_local(
    registry_root: PathBuf,
    reference: &str,
    capsule: &CapsuleManifest,
    capsule_bytes: &[u8],
    capsule_digest: &str,
    config_bytes: &[u8],
    config_digest: &str,
    manifest: &OciImageManifest,
    manifest_bytes: &[u8],
    manifest_digest: &str,
    push_layers: &[PushLayer],
    json: bool,
) -> Result<()> {
    let layout = OciLayout::ensure(registry_root.clone())?;
    layout.ensure_blob_from_bytes(config_digest, config_bytes)?;
    layout.ensure_blob_from_bytes(capsule_digest, capsule_bytes)?;

    let mut cas_stored = 0usize;
    let mut cas_reused = 0usize;
    for layer in push_layers {
        let stored = layout.ensure_blob_from_path(&layer.digest, &layer.source_path)?;
        if stored {
            cas_stored += 1;
        } else {
            cas_reused += 1;
        }
    }

    layout.ensure_blob_from_bytes(manifest_digest, manifest_bytes)?;

    let mut index = layout.load_index()?;
    index.manifests.retain(|entry| {
        entry
            .annotations
            .get("org.opencontainers.image.ref.name")
            .map(|value| value.as_str())
            != Some(reference)
    });
    let mut entry_annotations = HashMap::new();
    entry_annotations.insert(
        "org.opencontainers.image.ref.name".into(),
        reference.to_string(),
    );
    entry_annotations.insert(
        "com.adep.package.id".into(),
        capsule.package().id.to_string(),
    );
    index.manifests.push(OciIndexEntry {
        media_type: manifest.media_type.clone(),
        digest: manifest_digest.to_string(),
        size: manifest_bytes.len() as u64,
        annotations: entry_annotations,
    });
    layout.write_index(&index)?;

    let entry_count = capsule.entries().len();
    let summary = json!({
        "reference": reference,
        "registry": registry_root.display().to_string(),
        "entry_count": entry_count,
        "cas_stored": cas_stored,
        "cas_reused": cas_reused,
        "manifest_digest": manifest_digest,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "Pushed capsule '{}' with {} {} -> {}",
            reference,
            entry_count,
            if entry_count == 1 { "entry" } else { "entries" },
            registry_root.display()
        );
        println!(
            "  CAS blobs stored: {} | reused: {} | manifest: {}",
            cas_stored, cas_reused, manifest_digest
        );
        println!("  registry layout: {}", registry_root.display());
    }
    Ok(())
}

fn push_to_remote(
    destination: RemoteDestination,
    capsule: &CapsuleManifest,
    capsule_bytes: &[u8],
    capsule_digest: &str,
    config_bytes: &[u8],
    config_digest: &str,
    manifest: &OciImageManifest,
    manifest_bytes: &[u8],
    manifest_digest: &str,
    push_layers: &[PushLayer],
    json: bool,
) -> Result<()> {
    let client = RemoteClient::new(destination.clone())?;

    client.ensure_blob_from_bytes(config_digest, config_bytes)?;
    client.ensure_blob_from_bytes(capsule_digest, capsule_bytes)?;

    let mut cas_stored = 0usize;
    let mut cas_reused = 0usize;
    for layer in push_layers {
        if client.ensure_blob_from_path(&layer.digest, &layer.source_path)? {
            cas_stored += 1;
        } else {
            cas_reused += 1;
        }
    }

    let remote_digest = client.put_manifest(manifest, manifest_bytes)?;
    if remote_digest != *manifest_digest {
        bail!(
            "remote registry reported manifest digest {} but local digest is {}",
            remote_digest,
            manifest_digest
        );
    }

    let entry_count = capsule.entries().len();
    let resolved_reference = format!(
        "{}/{}@{}",
        destination.host.as_str(),
        destination.repository.as_str(),
        remote_digest
    );
    let summary = json!({
        "reference": destination.reference_string(),
        "registry": destination.display().to_string(),
        "entry_count": entry_count,
        "cas_stored": cas_stored,
        "cas_reused": cas_reused,
        "manifest_digest": remote_digest,
        "resolved_reference": resolved_reference,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "Pushed capsule '{}' with {} {} -> {}",
            destination.reference_string(),
            entry_count,
            if entry_count == 1 { "entry" } else { "entries" },
            destination.display()
        );
        println!(
            "  CAS blobs stored: {} | reused: {} | manifest: {}",
            cas_stored, cas_reused, remote_digest
        );
        println!("  resolved: {}", resolved_reference);
    }
    Ok(())
}

fn ingest_capsule_into_store<F>(
    capsule: &CapsuleManifest,
    store: &BlobStore,
    mut fetch_blob: F,
) -> Result<IngestReport>
where
    F: FnMut(&IndexEntry, &Path, &str) -> Result<()>,
{
    let blobs_dir = store.root().join("blobs");
    let verifier = Verifier::new();
    let mut report = IngestReport::default();

    for entry in capsule.entries() {
        package::ensure_safe_relative_path(Path::new(&entry.path))
            .with_context(|| format!("invalid entry path '{}'", entry.path))?;

        let compressed = entry
            .compressed_sha256
            .as_ref()
            .ok_or_else(|| anyhow!("capsule entry '{}' missing compressed_sha256", entry.path))?;
        let digest = format!("sha256:{compressed}");

        let dest_blob = blobs_dir.join(&entry.path);
        if dest_blob.exists() {
            report.reused += 1;
        } else {
            if let Some(parent) = dest_blob.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fetch_blob(entry, &dest_blob, &digest)?;
            report.stored += 1;
        }

        let compressed_hash = entry.compressed.as_ref().map(to_compressed_hash);
        let result = match verifier.verify(compressed_hash.clone(), &entry.raw_sha256, &dest_blob) {
            Ok(res) => res,
            Err(CasError::UnsupportedCompression(alg)) => {
                bail!(
                    "entry '{}' uses unsupported compression algorithm '{}'",
                    entry.path,
                    alg
                );
            }
            Err(CasError::CompressedHashMismatch(alg)) => {
                bail!(
                    "compressed hash mismatch for '{}' (algorithm: {})",
                    entry.path,
                    alg
                );
            }
            Err(CasError::HashMismatch) => {
                bail!(
                    "raw hash mismatch for '{}' – expected {}",
                    entry.path,
                    entry.raw_sha256
                );
            }
            Err(CasError::CompressionRatioExceeded {
                raw, compressed, ..
            }) => {
                bail!(
                    "compressed blob '{}' expands {} raw bytes vs {} compressed bytes (limit {:.0}x)",
                    entry.path,
                    raw,
                    compressed,
                    MAX_COMPRESSION_RATIO
                );
            }
            Err(err) => bail!("failed to verify '{}' – {}", entry.path, err),
        };

        if let Some(expected_size) = entry.size {
            if expected_size != result.verified_bytes {
                bail!(
                    "size mismatch for '{}' (expected {}, actual {})",
                    entry.path,
                    expected_size,
                    result.verified_bytes
                );
            }
        }
    }

    Ok(report)
}

struct RemoteClient {
    destination: RemoteDestination,
    http: HttpClient,
    auth_header: Option<HeaderValue>,
}

struct RemoteManifest {
    manifest: OciImageManifest,
    bytes: Vec<u8>,
    digest: String,
}

impl RemoteClient {
    fn new(destination: RemoteDestination) -> Result<Self> {
        let mut builder = HttpClient::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(format!("adep-cli/{}", env!("CARGO_PKG_VERSION")));

        if destination.scheme == "http" {
            if env::var("ADEP_REGISTRY_ALLOW_INSECURE").ok().as_deref() != Some("1") {
                bail!(
                    "refusing to connect to insecure registry {} without ADEP_REGISTRY_ALLOW_INSECURE=1",
                    destination.base_url
                );
            }
        } else if env::var("ADEP_REGISTRY_ALLOW_INVALID_CERTS")
            .ok()
            .as_deref()
            == Some("1")
        {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let http = builder
            .build()
            .context("failed to construct registry HTTP client")?;
        let auth_header = load_registry_auth_header()?;

        Ok(Self {
            destination,
            http,
            auth_header,
        })
    }

    fn with_auth(
        &self,
        builder: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        if let Some(value) = &self.auth_header {
            builder.header(AUTHORIZATION, value.clone())
        } else {
            builder
        }
    }

    fn blob_exists(&self, digest: &str) -> Result<bool> {
        let url = self.destination.blob_url(digest);
        let response = self
            .with_auth(self.http.head(url))
            .header(ACCEPT, "application/octet-stream")
            .send()
            .with_context(|| format!("failed to query blob {} on registry", digest))?;
        match response.status() {
            StatusCode::OK => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
                "registry authentication failed with status {} for blob {}",
                response.status(),
                digest
            ),
            other => bail!("unexpected status {} while querying blob {}", other, digest),
        }
    }

    fn ensure_blob_from_bytes(&self, digest: &str, bytes: &[u8]) -> Result<bool> {
        if self.blob_exists(digest)? {
            return Ok(false);
        }
        let url = self.destination.upload_url(digest);
        let response = self
            .with_auth(self.http.post(url))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .with_context(|| format!("failed to upload blob {} to registry", digest))?;
        match response.status() {
            StatusCode::CREATED | StatusCode::ACCEPTED => Ok(true),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
                "registry authentication failed with status {} while uploading {}",
                response.status(),
                digest
            ),
            other => bail!(
                "registry returned status {} while uploading {}",
                other,
                digest
            ),
        }
    }

    fn ensure_blob_from_path(&self, digest: &str, source: &Path) -> Result<bool> {
        if self.blob_exists(digest)? {
            return Ok(false);
        }
        let size = fs::metadata(source)
            .with_context(|| format!("failed to stat {}", source.display()))?
            .len();
        let file =
            File::open(source).with_context(|| format!("failed to open {}", source.display()))?;
        let body = Body::sized(file, size);
        let url = self.destination.upload_url(digest);
        let response = self
            .with_auth(self.http.post(url))
            .header(CONTENT_TYPE, "application/octet-stream")
            .body(body)
            .send()
            .with_context(|| {
                format!(
                    "failed to upload blob {} from {} to registry",
                    digest,
                    source.display()
                )
            })?;
        match response.status() {
            StatusCode::CREATED | StatusCode::ACCEPTED => Ok(true),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
                "registry authentication failed with status {} while uploading {}",
                response.status(),
                digest
            ),
            other => bail!(
                "registry returned status {} while uploading {}",
                other,
                digest
            ),
        }
    }

    fn put_manifest(&self, manifest: &OciImageManifest, bytes: &[u8]) -> Result<String> {
        let url = self.destination.manifest_url();
        let response = self
            .with_auth(self.http.put(url))
            .header(CONTENT_TYPE, manifest.media_type.as_str())
            .body(bytes.to_vec())
            .send()
            .with_context(|| {
                format!(
                    "failed to upload manifest for reference {}",
                    self.destination.reference_string()
                )
            })?;
        match response.status() {
            StatusCode::CREATED | StatusCode::ACCEPTED | StatusCode::OK => {}
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
                "registry authentication failed with status {} while uploading manifest",
                response.status()
            ),
            other => bail!(
                "registry returned status {} while uploading manifest",
                other
            ),
        }
        let digest = response
            .headers()
            .get("Docker-Content-Digest")
            .and_then(|value| value.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| sha256_hex(bytes));
        Ok(digest)
    }

    fn fetch_manifest(&self) -> Result<RemoteManifest> {
        let url = self.destination.manifest_url();
        let response = self
            .with_auth(self.http.get(url))
            .header(
                ACCEPT,
                "application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json",
            )
            .send()
            .with_context(|| {
                format!(
                    "failed to fetch manifest for reference {}",
                    self.destination.reference_string()
                )
            })?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
                "registry authentication failed with status {} while fetching manifest",
                response.status()
            ),
            other => bail!("registry returned status {} while fetching manifest", other),
        }
        let headers = response.headers().clone();
        let bytes = response
            .bytes()
            .context("failed to read manifest bytes from registry")?
            .to_vec();
        let manifest: OciImageManifest =
            serde_json::from_slice(&bytes).context("failed to parse registry manifest")?;
        let digest = headers
            .get("Docker-Content-Digest")
            .and_then(|value| value.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| sha256_hex(&bytes));
        Ok(RemoteManifest {
            manifest,
            bytes,
            digest,
        })
    }

    fn download_blob_bytes(&self, digest: &str) -> Result<Vec<u8>> {
        let url = self.destination.blob_url(digest);
        let response = self
            .with_auth(self.http.get(url))
            .send()
            .with_context(|| format!("failed to download blob {}", digest))?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
                "registry authentication failed with status {} while downloading {}",
                response.status(),
                digest
            ),
            StatusCode::NOT_FOUND => bail!("blob {} missing on registry", digest),
            other => bail!(
                "registry returned status {} while downloading {}",
                other,
                digest
            ),
        }
        let bytes = response
            .bytes()
            .context("failed to read blob bytes from registry")?
            .to_vec();
        Ok(bytes)
    }

    fn download_blob_to_path(&self, digest: &str, dest: &Path) -> Result<()> {
        let url = self.destination.blob_url(digest);
        let mut response = self.with_auth(self.http.get(url)).send().with_context(|| {
            format!("failed to download blob {} into {}", digest, dest.display())
        })?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!(
                "registry authentication failed with status {} while downloading {}",
                response.status(),
                digest
            ),
            StatusCode::NOT_FOUND => bail!("blob {} missing on registry", digest),
            other => bail!(
                "registry returned status {} while downloading {}",
                other,
                digest
            ),
        }
        let mut file =
            File::create(dest).with_context(|| format!("failed to create {}", dest.display()))?;
        std::io::copy(&mut response, &mut file)
            .with_context(|| format!("failed to write blob {} into {}", digest, dest.display()))?;
        Ok(())
    }
}

fn run_pull(args: &PullArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("failed to resolve current_directory")?;
    let target = resolve_registry_target(&cwd, &args.registry, &args.reference)?;
    match target {
        RegistryTarget::Local(registry_root) => pull_from_local(registry_root, args),
        RegistryTarget::Remote(destination) => pull_from_remote(destination, args),
    }
}

fn pull_from_local(registry_root: PathBuf, args: &PullArgs) -> Result<()> {
    if !registry_root.exists() {
        bail!(
            "registry {} does not exist; run 'adep deps push' first",
            registry_root.display()
        );
    }
    let layout = OciLayout::ensure(registry_root.clone())?;
    let index = layout.load_index()?;
    let manifest_digest =
        resolve_manifest_reference(&index, &args.reference).with_context(|| {
            format!(
                "failed to resolve reference '{}' in {}",
                args.reference,
                registry_root.display()
            )
        })?;
    let manifest_bytes = layout.read_blob(&manifest_digest)?;
    let manifest: OciImageManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| "failed to parse registry manifest")?;
    let capsule_layer = manifest
        .layers
        .iter()
        .find(|layer| layer.media_type == "application/vnd.adep.capsule.manifest.v1+json")
        .ok_or_else(|| anyhow!("capsule manifest layer missing in OCI artifact"))?;
    let capsule_bytes = layout.read_blob(&capsule_layer.digest)?;
    let capsule: CapsuleManifest = serde_json::from_slice(&capsule_bytes)
        .with_context(|| "failed to parse capsule manifest layer")?;

    let cas_root = match &args.cas_dir {
        Some(dir) => resolve_root(dir),
        None => default_cas_root(),
    }?;
    let store = BlobStore::open(&cas_root)
        .with_context(|| format!("failed to open CAS root {}", cas_root.display()))?;
    let output_path = match &args.output {
        Some(path) => resolve_path(store.root(), path),
        None => store.root().join("capsule-manifest.json"),
    };

    let report = ingest_capsule_into_store(&capsule, &store, |entry, dest, digest| {
        let source_blob = layout
            .blob_path(digest)
            .with_context(|| format!("failed to resolve blob path for {}", digest))?;
        if !source_blob.exists() {
            bail!(
                "registry blob '{}' missing at {}",
                digest,
                source_blob.display()
            );
        }
        fs::copy(&source_blob, dest).with_context(|| {
            format!(
                "failed to copy blob {} into local CAS at {}",
                entry.path,
                dest.display()
            )
        })?;
        Ok(())
    })?;

    finalize_pull(
        &registry_root.display().to_string(),
        &manifest_digest,
        manifest_bytes.len() as u64,
        &capsule,
        &store,
        &output_path,
        report,
        args,
    )
}

fn pull_from_remote(destination: RemoteDestination, args: &PullArgs) -> Result<()> {
    let client = RemoteClient::new(destination.clone())?;
    let RemoteManifest {
        manifest,
        bytes: manifest_bytes,
        digest: manifest_digest,
    } = client.fetch_manifest()?;
    let capsule_layer = manifest
        .layers
        .iter()
        .find(|layer| layer.media_type == "application/vnd.adep.capsule.manifest.v1+json")
        .ok_or_else(|| anyhow!("capsule manifest layer missing in OCI artifact"))?;
    let capsule_bytes = client.download_blob_bytes(&capsule_layer.digest)?;
    let capsule: CapsuleManifest = serde_json::from_slice(&capsule_bytes)
        .with_context(|| "failed to parse capsule manifest layer")?;

    let cas_root = match &args.cas_dir {
        Some(dir) => resolve_root(dir),
        None => default_cas_root(),
    }?;
    let store = BlobStore::open(&cas_root)
        .with_context(|| format!("failed to open CAS root {}", cas_root.display()))?;
    let output_path = match &args.output {
        Some(path) => resolve_path(store.root(), path),
        None => store.root().join("capsule-manifest.json"),
    };

    let mut descriptors = HashMap::new();
    for layer in &manifest.layers {
        if let Some(annotations) = &layer.annotations {
            if let Some(path) = annotations.get("org.opencontainers.image.title") {
                descriptors.insert(path.clone(), layer.clone());
            }
        }
    }

    let report = ingest_capsule_into_store(&capsule, &store, |entry, dest, digest| {
        let descriptor = descriptors.get(&entry.path).ok_or_else(|| {
            anyhow!(
                "registry manifest missing descriptor for capsule entry '{}'",
                entry.path
            )
        })?;
        if descriptor.digest != digest {
            bail!(
                "descriptor digest mismatch for '{}': manifest has {}, capsule expects {}",
                entry.path,
                descriptor.digest,
                digest
            );
        }
        client
            .download_blob_to_path(&descriptor.digest, dest)
            .with_context(|| {
                format!(
                    "failed to download blob {} into {}",
                    descriptor.digest,
                    dest.display()
                )
            })
    })?;

    let registry_display = destination.display().to_string();
    finalize_pull(
        &registry_display,
        &manifest_digest,
        manifest_bytes.len() as u64,
        &capsule,
        &store,
        &output_path,
        report,
        args,
    )
}

fn finalize_pull(
    registry_display: &str,
    manifest_digest: &str,
    manifest_size: u64,
    capsule: &CapsuleManifest,
    store: &BlobStore,
    output_path: &Path,
    report: IngestReport,
    args: &PullArgs,
) -> Result<()> {
    let index_path = store.root().join("index.json");
    let current_index = if index_path.exists() {
        let file = File::open(&index_path)
            .with_context(|| format!("failed to open {}", index_path.display()))?;
        CanonicalIndex::from_reader(file)?
    } else {
        CanonicalIndex::new(Vec::new())
    };
    let merge_report = current_index.merge(capsule.entries().to_vec())?;
    if !merge_report.conflicts.is_empty() {
        bail!(format_merge_conflicts(&merge_report));
    }
    if !merge_report.added.is_empty() || !merge_report.updated.is_empty() || !index_path.exists() {
        write_index_atomically(&index_path, &merge_report.index)
            .with_context(|| format!("failed to update {}", index_path.display()))?;
    }

    write_json_atomically(output_path, capsule)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    let entry_count = capsule.entries().len();
    let summary = json!({
        "reference": args.reference,
        "registry": registry_display,
        "cas_root": store.root().display().to_string(),
        "capsule": output_path.display().to_string(),
        "entry_count": entry_count,
        "stored_blobs": report.stored,
        "reused_blobs": report.reused,
        "manifest_digest": manifest_digest,
        "manifest_size": manifest_size,
        "index": {
            "path": index_path.display().to_string(),
            "added": merge_report.added.len(),
            "updated": merge_report.updated.len(),
            "unchanged": merge_report.unchanged.len(),
        },
    });
    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "Pulled capsule '{}' into {} ({} {}, stored {}, reused {})",
            args.reference,
            store.root().display(),
            entry_count,
            if entry_count == 1 { "entry" } else { "entries" },
            report.stored,
            report.reused
        );
        println!(
            "  registry: {} | manifest digest: {}",
            registry_display, manifest_digest
        );
        println!(
            "  index: {} (added {}, updated {}, unchanged {})",
            index_path.display(),
            merge_report.added.len(),
            merge_report.updated.len(),
            merge_report.unchanged.len()
        );
    }
    Ok(())
}

fn run_verify(args: &VerifyArgs) -> Result<()> {
    let root = resolve_root(&args.root)?;
    let manifest = if args.index.is_none() || args.blobs_dir.is_none() {
        let manifest_path = root.join("manifest.json");
        Manifest::load(&manifest_path)
            .with_context(|| format!("failed to load {}", manifest_path.display()))?
            .into()
    } else {
        None
    };

    let index_path = match &args.index {
        Some(path) => resolve_path(&root, path),
        None => infer_default_path(&root, manifest.as_ref(), "cas/index.json")?,
    };
    let blobs_base = match &args.blobs_dir {
        Some(path) => resolve_path(&root, path),
        None => infer_default_path(&root, manifest.as_ref(), "cas/blobs")?,
    };

    let file = File::open(&index_path)
        .with_context(|| format!("failed to open index {}", index_path.display()))?;
    let index = CanonicalIndex::from_reader(file)?;
    if index.is_empty() {
        println!("Index is empty – nothing to verify.");
        return Ok(());
    }

    let duplicates = index.duplicates();
    if !duplicates.is_empty() {
        let mut messages = Vec::new();
        for duplicate in duplicates {
            let key = match duplicate.kind {
                DuplicateKind::Path(path) => format!("path '{path}'"),
                DuplicateKind::Coord(coord) => format!("coord '{coord}'"),
                DuplicateKind::CompressedSha256(digest) => {
                    format!("compressed_sha256 '{digest}'")
                }
                DuplicateKind::RawSha256(digest) => format!("raw_sha256 '{digest}'"),
            };
            let paths = duplicate
                .entries
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            messages.push(format!("{key} overlaps entries at paths: {paths}"));
        }
        return Err(anyhow!(
            "index contains conflicting entries: {}",
            messages.join("; ")
        ));
    }

    let verifier = Verifier::new();
    let mut verified = 0usize;
    let mut total_bytes = 0u64;
    let mut reports = Vec::new();
    for entry in index.entries() {
        let entry_path_str = &entry.path;
        package::ensure_safe_relative_path(Path::new(&entry.path))
            .with_context(|| format!("invalid entry path '{}': outside CAS root", entry.path))?;
        let entry_path = blobs_base.join(Path::new(&entry.path));
        let compressed_info = entry.compressed.as_ref();
        let compressed = match (compressed_info, entry.compressed_sha256.as_ref()) {
            (Some(info), Some(_)) => Some(to_compressed_hash(info)),
            (Some(_), None) => {
                return Err(anyhow!(
                    "entry '{}' missing compressed_sha256 while compression metadata present",
                    entry_path_str
                ))
            }
            (None, Some(_)) => {
                return Err(anyhow!(
                    "entry '{}' missing compression metadata for compressed_sha256",
                    entry_path_str
                ))
            }
            (None, None) => None,
        };
        let result = match verifier.verify(compressed, &entry.raw_sha256, &entry_path) {
            Ok(res) => res,
            Err(CasError::UnsupportedCompression(alg)) => {
                return Err(anyhow!(
                    "entry '{}' uses unsupported compression algorithm '{}'; supported: zstd",
                    entry_path_str,
                    alg
                ));
            }
            Err(CasError::CompressedHashMismatch(alg)) => {
                return Err(anyhow!(
                    "compressed hash mismatch for '{}' (algorithm: {}) – check index compressed_sha256",
                    entry_path_str,
                    alg
                ));
            }
            Err(CasError::HashMismatch) => {
                return Err(anyhow!(
                    "raw hash mismatch for '{}' – expected {}",
                    entry_path_str,
                    &entry.raw_sha256
                ));
            }
            Err(CasError::CompressionRatioExceeded {
                raw, compressed, ..
            }) => {
                let ratio = if compressed == 0 {
                    f64::INFINITY
                } else {
                    raw as f64 / compressed as f64
                };
                return Err(anyhow!(
                    "compressed blob '{}' expands {:.1}x ({} raw bytes vs {} compressed bytes; limit {:.0}x)",
                    entry_path_str,
                    ratio,
                    raw,
                    compressed,
                    MAX_COMPRESSION_RATIO
                ));
            }
            Err(CasError::Decompression(msg)) => {
                let alg = compressed_info.map(|c| c.alg.as_str()).unwrap_or("unknown");
                return Err(anyhow!(
                    "failed to decompress '{}' with {}: {}",
                    entry_path_str,
                    alg,
                    msg
                ));
            }
            Err(err) => {
                return Err(anyhow!("failed to verify '{}' – {}", entry_path_str, err));
            }
        };
        if let Some(expected_size) = entry.size {
            if expected_size != result.verified_bytes {
                return Err(anyhow!(
                    "size mismatch for '{}' (expected {}, actual {})",
                    entry_path_str,
                    expected_size,
                    result.verified_bytes
                ));
            }
        }
        if !args.json {
            let compressed_bytes = std::fs::metadata(&entry_path).map(|meta| meta.len()).ok();
            if let Some(comp) = compressed_info {
                match compressed_bytes {
                    Some(size) => println!(
                        "✓ {} — raw {} bytes ({} compressed bytes via {})",
                        entry_path_str, result.verified_bytes, size, comp.alg
                    ),
                    None => println!(
                        "✓ {} — raw {} bytes (compressed via {})",
                        entry_path_str, result.verified_bytes, comp.alg
                    ),
                }
            } else {
                println!("✓ {} — raw {} bytes", entry_path_str, result.verified_bytes);
            }
        }
        verified += 1;
        total_bytes += result.verified_bytes;
        reports.push(json!({
            "path": entry_path_str,
            "bytes": result.verified_bytes
        }));
    }

    if args.json {
        let summary = json!({
            "count": verified,
            "total_bytes": total_bytes,
            "entries": reports,
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "Verified {} CAS blob{} | total raw bytes: {} | index: {}",
            verified,
            if verified == 1 { "" } else { "s" },
            total_bytes,
            index_path.display()
        );
    }
    Ok(())
}

fn run_vendor(args: &VendorArgs) -> Result<()> {
    let source_path = absolutize(&args.source)?;
    let metadata = fs::metadata(&source_path).with_context(|| {
        format!(
            "failed to read artifact metadata for {}",
            source_path.display()
        )
    })?;
    if !metadata.is_file() {
        bail!("source '{}' must be a regular file", source_path.display());
    }

    let cas_root = match &args.cas_dir {
        Some(dir) => absolutize(dir)?,
        None => default_cas_root()?,
    };

    let store = BlobStore::open(&cas_root)
        .with_context(|| format!("failed to open CAS root {}", cas_root.display()))?;

    let outcome = match args.lang {
        VendorLanguage::Python => ingest_python_wheel(&store, &source_path, args)?,
        VendorLanguage::Node => ingest_pnpm_tarball(&store, &source_path, args)?,
    };

    let index_path = cas_root.join("index.json");
    let current_index = if index_path.exists() {
        let file = File::open(&index_path)
            .with_context(|| format!("failed to open {}", index_path.display()))?;
        CanonicalIndex::from_reader(file)?
    } else {
        CanonicalIndex::new(Vec::new())
    };

    let merge_report = current_index.merge(vec![outcome.index_entry.clone()])?;
    if !merge_report.conflicts.is_empty() {
        bail!(format_merge_conflicts(&merge_report));
    }

    if !merge_report.added.is_empty() || !merge_report.updated.is_empty() || !index_path.exists() {
        write_index_atomically(&index_path, &merge_report.index)?;
    }

    let mut stdout = std::io::stdout();
    emit_vendor_summary(
        args,
        &cas_root,
        &index_path,
        &outcome,
        &merge_report,
        &mut stdout,
    )?;
    Ok(())
}

fn ingest_python_wheel(
    store: &BlobStore,
    source: &Path,
    args: &VendorArgs,
) -> Result<VendorOutcome> {
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("invalid wheel filename: {}", source.display()))?;
    let mut wheel = parse_wheel_filename(file_name)
        .ok_or_else(|| anyhow!("failed to parse wheel filename '{}'", file_name))?;
    load_wheel_metadata(source, &mut wheel)?;
    let coords = determine_python_coords(&args.coords, &wheel)?;
    let platform_tags = unique_sorted(match &args.platform {
        Some(tag) => vec![tag.clone()],
        None => wheel.platform_tags.clone(),
    });
    wheel.platform_tags = platform_tags.clone();
    wheel.requires_dist.sort();

    let blob = store.ingest_path(source, None)?;
    let metadata = IndexMetadata {
        filename: Some(file_name.to_string()),
        kind: Some("python-wheel".to_string()),
    };
    let entry =
        build_index_entry_from_blob(&blob, vec![coords.clone()], platform_tags, Some(metadata));

    Ok(VendorOutcome {
        artifact: ArtifactMetadata::PythonWheel(wheel),
        blob,
        index_entry: entry,
    })
}

fn ingest_pnpm_tarball(
    store: &BlobStore,
    source: &Path,
    args: &VendorArgs,
) -> Result<VendorOutcome> {
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("invalid tarball filename: {}", source.display()))?;
    let mut meta = parse_pnpm_filename(file_name)
        .ok_or_else(|| anyhow!("failed to parse tarball filename '{}'", file_name))?;
    let package_json = read_pnpm_package_json(source)?;
    let PackageJson {
        name,
        version,
        description,
        license,
        dependencies,
    } = package_json;
    if normalize_npm_name(&meta.package_name) != normalize_npm_name(&name) {
        bail!(
            "tarball name '{}' does not match package.json name '{}'",
            meta.package_name,
            name
        );
    }
    if meta.version != version {
        bail!(
            "tarball version '{}' does not match package.json version '{}'",
            meta.version,
            version
        );
    }
    meta.package_name = name;
    meta.version = version;
    meta.description = description;
    meta.license = license;
    meta.dependencies = dependencies;
    let coords = determine_pnpm_coords(&args.coords, &meta)?;
    let platform_tags = unique_sorted(match &args.platform {
        Some(tag) => vec![tag.clone()],
        None => Vec::new(),
    });

    let blob = store.ingest_path(source, None)?;
    let metadata = IndexMetadata {
        filename: Some(file_name.to_string()),
        kind: Some("pnpm-tarball".to_string()),
    };
    let entry =
        build_index_entry_from_blob(&blob, vec![coords.clone()], platform_tags, Some(metadata));

    Ok(VendorOutcome {
        artifact: ArtifactMetadata::PnpmTarball(meta),
        blob,
        index_entry: entry,
    })
}

fn determine_coords(provided: &Option<String>, fallback: String) -> Result<String> {
    if let Some(value) = provided {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            bail!("--coords must not be empty");
        }
        Ok(trimmed.to_string())
    } else {
        Ok(fallback)
    }
}

fn build_index_entry_from_blob(
    blob: &StoredBlob,
    coords: Vec<String>,
    platform: Vec<String>,
    metadata: Option<IndexMetadata>,
) -> IndexEntry {
    let mut entry = IndexEntry::default();
    entry.path = format!("sha256-{}", blob.compressed_sha256);
    entry.raw_sha256 = blob.raw_sha256.clone();
    entry.size = Some(blob.raw_size);
    entry.compressed_sha256 = Some(blob.compressed_sha256.clone());
    entry.coords = coords;
    entry.platform = platform;
    entry.metadata = metadata;
    entry.compressed = Some(CompressedEntry {
        alg: "zstd".into(),
        size: Some(blob.compressed_size),
        digest: Some(blob.compressed_sha256.clone()),
    });
    entry
}

fn unique_sorted(mut values: Vec<String>) -> Vec<String> {
    values.retain(|v| !v.trim().is_empty());
    values.sort();
    values.dedup();
    values
}

fn parse_wheel_filename(filename: &str) -> Option<WheelMetadata> {
    if !filename.ends_with(".whl") {
        return None;
    }
    let stem = &filename[..filename.len() - 4];
    let parts: Vec<&str> = stem.split('-').collect();
    if parts.len() < 5 {
        return None;
    }
    let distribution = parts.get(0)?.to_string();
    let version = parts.get(1)?.to_string();
    let python_tag = parts.get(parts.len() - 3)?.to_string();
    let abi_tag = parts.get(parts.len() - 2)?.to_string();
    let platform_part = parts.get(parts.len() - 1)?.to_string();
    let build_tag = if parts.len() > 5 {
        Some(parts.get(parts.len() - 4)?.to_string())
    } else {
        None
    };
    let platform_tags = platform_part
        .split('.')
        .map(|tag| tag.to_string())
        .collect();
    Some(WheelMetadata {
        distribution: distribution.clone(),
        version,
        build_tag,
        python_tag,
        abi_tag,
        platform_tags,
        metadata_name: distribution,
        metadata_version: None,
        summary: None,
        requires_python: None,
        requires_dist: Vec::new(),
    })
}

fn parse_pnpm_filename(filename: &str) -> Option<PnpmMetadata> {
    if !(filename.ends_with(".tgz") || filename.ends_with(".tar.gz")) {
        return None;
    }
    let trimmed = filename
        .trim_end_matches(".tgz")
        .trim_end_matches(".tar.gz");
    let idx = trimmed.rfind('-')?;
    let package_name = trimmed[..idx].to_string();
    let version = trimmed[idx + 1..].to_string();
    if package_name.is_empty() || version.is_empty() {
        return None;
    }
    Some(PnpmMetadata {
        package_name,
        version,
        description: None,
        license: None,
        dependencies: BTreeMap::new(),
    })
}

fn load_wheel_metadata(path: &Path, wheel: &mut WheelMetadata) -> Result<()> {
    let file =
        File::open(path).with_context(|| format!("failed to open wheel {}", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("invalid wheel archive {}", path.display()))?;
    let mut metadata_buf = String::new();
    let mut found = false;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("failed to read entry {i} from {}", path.display()))?;
        let entry_path = Path::new(entry.name());
        ensure_archive_member_safe(entry_path).map_err(|err| {
            anyhow!(
                "wheel '{}' contains unsafe entry '{}': {}",
                path.display(),
                entry.name(),
                err
            )
        })?;
        let is_metadata = entry_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.eq_ignore_ascii_case("METADATA"))
            .unwrap_or(false);
        let parent_ok = entry_path
            .parent()
            .and_then(|parent| parent.to_str())
            .map(|parent| parent.ends_with(".dist-info"))
            .unwrap_or(false);
        if is_metadata && parent_ok {
            entry
                .read_to_string(&mut metadata_buf)
                .with_context(|| format!("failed to read METADATA from {}", path.display()))?;
            found = true;
            break;
        }
    }
    if !found {
        bail!(
            "wheel '{}' missing *.dist-info/METADATA entry",
            path.display()
        );
    }
    let parsed = parse_wheel_metadata_fields(&metadata_buf)?;
    if normalize_pypi_name(&wheel.distribution) != normalize_pypi_name(&parsed.name) {
        bail!(
            "wheel filename distribution '{}' does not match METADATA Name '{}'",
            wheel.distribution,
            parsed.name
        );
    }
    if wheel.version != parsed.version {
        bail!(
            "wheel filename version '{}' does not match METADATA Version '{}'",
            wheel.version,
            parsed.version
        );
    }
    wheel.metadata_name = parsed.name;
    wheel.metadata_version = parsed.metadata_version;
    wheel.summary = parsed.summary;
    wheel.requires_python = parsed.requires_python;
    wheel.requires_dist = parsed.requires_dist;
    Ok(())
}

fn parse_wheel_metadata_fields(contents: &str) -> Result<ParsedWheelMetadata> {
    let table = parse_metadata_key_values(contents);
    let name = table
        .get("Name")
        .and_then(|values| values.first())
        .cloned()
        .ok_or_else(|| anyhow!("wheel METADATA missing Name field"))?;
    let version = table
        .get("Version")
        .and_then(|values| values.first())
        .cloned()
        .ok_or_else(|| anyhow!("wheel METADATA missing Version field"))?;
    let metadata_version = table
        .get("Metadata-Version")
        .and_then(|values| values.first())
        .cloned();
    let summary = table
        .get("Summary")
        .and_then(|values| values.first())
        .cloned()
        .filter(|value| !value.is_empty());
    let requires_python = table
        .get("Requires-Python")
        .and_then(|values| values.first())
        .cloned()
        .filter(|value| !value.is_empty());
    let mut requires_dist = table
        .get("Requires-Dist")
        .map(|values| values.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    requires_dist.sort();
    requires_dist.dedup();
    Ok(ParsedWheelMetadata {
        name,
        version,
        metadata_version,
        summary,
        requires_python,
        requires_dist,
    })
}

fn parse_metadata_key_values(contents: &str) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current_key: Option<String> = None;
    let mut current_value = String::new();

    for line in contents.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(_) = current_key {
                if !current_value.is_empty() {
                    current_value.push(' ');
                }
                current_value.push_str(line.trim());
            }
            continue;
        }
        if let Some(key) = current_key.take() {
            map.entry(key)
                .or_default()
                .push(current_value.trim().to_string());
            current_value.clear();
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Some((key, rest)) = line.split_once(':') {
            current_key = Some(key.trim().to_string());
            current_value = rest.trim().to_string();
        }
    }
    if let Some(key) = current_key {
        map.entry(key)
            .or_default()
            .push(current_value.trim().to_string());
    }
    map
}

fn determine_python_coords(provided: &Option<String>, wheel: &WheelMetadata) -> Result<String> {
    let fallback = format!(
        "pkg:pypi/{}@{}",
        normalize_pypi_name(&wheel.metadata_name),
        wheel.version
    );
    let coords = determine_coords(provided, fallback)?;
    validate_python_coords(&coords, wheel)?;
    Ok(coords)
}

fn validate_python_coords(coords: &str, wheel: &WheelMetadata) -> Result<()> {
    if !coords.starts_with("pkg:pypi/") {
        bail!(
            "coords '{}' must start with pkg:pypi/ for Python artifacts",
            coords
        );
    }
    let rest = &coords["pkg:pypi/".len()..];
    let (name_part, version_part) = rest
        .rsplit_once('@')
        .ok_or_else(|| anyhow!("coords '{}' must include '@' separator", coords))?;
    if version_part != wheel.version {
        bail!(
            "coords version '{}' does not match wheel version '{}'",
            version_part,
            wheel.version
        );
    }
    if normalize_pypi_name(name_part) != normalize_pypi_name(&wheel.metadata_name) {
        bail!(
            "coords name '{}' does not match wheel METADATA name '{}'",
            name_part,
            wheel.metadata_name
        );
    }
    Ok(())
}

fn read_pnpm_package_json(path: &Path) -> Result<PackageJson> {
    let file =
        File::open(path).with_context(|| format!("failed to open tarball {}", path.display()))?;
    let mut decoder = GzDecoder::new(file);
    let mut header = [0u8; 512];
    loop {
        match decoder.read_exact(&mut header) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                bail!(
                    "tarball '{}' terminated unexpectedly before package.json",
                    path.display()
                );
            }
            Err(err) => return Err(err.into()),
        }
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let entry_name = parse_tar_entry_name(&header)?;
        let entry_size = parse_tar_entry_size(&header)?;
        let mut data = vec![0u8; entry_size as usize];
        decoder
            .read_exact(&mut data)
            .with_context(|| format!("failed to read entry data for {}", entry_name))?;
        let padding = (512 - (entry_size % 512)) % 512;
        if padding > 0 {
            let mut skip = vec![0u8; padding as usize];
            decoder.read_exact(&mut skip)?;
        }
        let entry_path = Path::new(&entry_name);
        ensure_archive_member_safe(entry_path).map_err(|err| {
            anyhow!(
                "tarball '{}' contains unsafe entry '{}': {}",
                path.display(),
                entry_name,
                err
            )
        })?;
        if entry_name == "package/package.json" || entry_name == "package.json" {
            let parsed: PackageJson = serde_json::from_slice(&data)
                .with_context(|| "failed to parse package.json contents")?;
            return Ok(parsed);
        }
    }
    bail!(
        "tarball '{}' does not contain package/package.json",
        path.display()
    );
}

fn parse_tar_entry_name(header: &[u8]) -> Result<String> {
    let name_field = &header[0..100];
    let end = name_field
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(name_field.len());
    let raw = &name_field[..end];
    let name = std::str::from_utf8(raw)
        .map_err(|_| anyhow!("invalid tar entry name"))?
        .trim_end()
        .to_string();
    Ok(name)
}

fn parse_tar_entry_size(header: &[u8]) -> Result<u64> {
    parse_tar_octal(&header[124..136])
}

fn parse_tar_octal(field: &[u8]) -> Result<u64> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    let value_str = std::str::from_utf8(&field[..end])
        .map_err(|_| anyhow!("invalid tar octal field"))?
        .trim();
    if value_str.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(value_str, 8)
        .map_err(|err| anyhow!("invalid tar octal value '{}': {}", value_str, err))
}

fn determine_pnpm_coords(provided: &Option<String>, meta: &PnpmMetadata) -> Result<String> {
    let fallback = format!(
        "pkg:npm/{}@{}",
        normalize_npm_name(&meta.package_name),
        meta.version
    );
    let coords = determine_coords(provided, fallback)?;
    validate_pnpm_coords(&coords, &meta.package_name, &meta.version)?;
    Ok(coords)
}

fn validate_pnpm_coords(coords: &str, package_name: &str, version: &str) -> Result<()> {
    if !coords.starts_with("pkg:npm/") {
        bail!(
            "coords '{}' must start with pkg:npm/ for pnpm artifacts",
            coords
        );
    }
    let rest = &coords["pkg:npm/".len()..];
    let (name_part, version_part) = rest
        .rsplit_once('@')
        .ok_or_else(|| anyhow!("coords '{}' must include '@' separator", coords))?;
    if version_part != version {
        bail!(
            "coords version '{}' does not match package.json version '{}'",
            version_part,
            version
        );
    }
    if normalize_npm_name(name_part) != normalize_npm_name(package_name) {
        bail!(
            "coords name '{}' does not match package.json name '{}'",
            name_part,
            package_name
        );
    }
    Ok(())
}

fn normalize_pypi_name(name: &str) -> String {
    name.replace('_', "-").to_lowercase()
}

fn normalize_npm_name(name: &str) -> String {
    name.replace('_', "-").to_lowercase()
}

fn default_cas_root() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow!("failed to locate home directory for CAS"))?;
    Ok(home.join(".adep").join("cas"))
}

fn default_deps_key_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("failed to locate home directory for dependency signing key"))?;
    Ok(home.join(".adep").join("keys").join("deps.json"))
}

fn absolutize(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let cwd = env::current_dir().context("failed to resolve current directory")?;
        Ok(cwd.join(path))
    }
}

fn write_index_atomically(path: &Path, index: &CanonicalIndex) -> Result<()> {
    let json = index.to_canonical_json()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(json.as_bytes())?;
    temp.flush()?;
    temp.persist(path)?;
    Ok(())
}

fn write_json_atomically<S: Serialize>(path: &Path, value: &S) -> Result<()> {
    let rendered = serde_json::to_string_pretty(value)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(rendered.as_bytes())?;
    temp.write_all(b"\n")?;
    temp.flush()?;
    temp.persist(path)?;
    Ok(())
}

fn load_capsule_manifest(path: &Path) -> Result<CapsuleManifest> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read capsule manifest {}", path.display()))?;
    let manifest: CapsuleManifest = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse capsule manifest {}", path.display()))?;
    manifest.validate()?;
    Ok(manifest)
}

fn format_merge_conflicts(report: &MergeReport) -> String {
    let mut parts = Vec::new();
    for conflict in &report.conflicts {
        let kind = match conflict.kind {
            MergeConflictKind::Path => "path",
            MergeConflictKind::Coord => "coord",
            MergeConflictKind::CompressedSha256 => "compressed_sha256",
            MergeConflictKind::RawSha256 => "raw_sha256",
        };
        parts.push(format!(
            "conflict on {} '{}' (existing path: {}, incoming path: {})",
            kind, conflict.key, conflict.existing.path, conflict.incoming.path
        ));
    }
    format!("index merge conflict: {}", parts.join("; "))
}

fn emit_vendor_summary(
    args: &VendorArgs,
    cas_root: &Path,
    index_path: &Path,
    outcome: &VendorOutcome,
    merge_report: &MergeReport,
    writer: &mut dyn Write,
) -> Result<()> {
    let coords = outcome
        .index_entry
        .coords
        .get(0)
        .cloned()
        .unwrap_or_else(|| "-".into());
    let platform = if outcome.index_entry.platform.is_empty() {
        String::from("-")
    } else {
        outcome.index_entry.platform.join(",")
    };
    let index_action = if !merge_report.added.is_empty() {
        "added"
    } else if !merge_report.updated.is_empty() {
        "updated"
    } else {
        "unchanged"
    };
    let status = match outcome.blob.status {
        BlobStatus::Stored => "stored",
        BlobStatus::Reused => "reused",
    };
    if args.json {
        let summary = json!({
            "artifact": outcome.artifact.to_json(),
            "blob": {
                "status": status,
                "compressed_sha256": outcome.blob.compressed_sha256,
                "compressed_size": outcome.blob.compressed_size,
                "raw_sha256": outcome.blob.raw_sha256,
                "raw_size": outcome.blob.raw_size,
                "path": outcome.index_entry.path,
            },
            "index_entry": {
                "coords": outcome.index_entry.coords,
                "platform": outcome.index_entry.platform,
                "compressed_sha256": outcome.index_entry.compressed_sha256,
                "raw_sha256": outcome.index_entry.raw_sha256,
                "size": outcome.index_entry.size,
            },
            "index_update": {
                "action": index_action,
                "added": merge_report.added.len(),
                "updated": merge_report.updated.len(),
                "unchanged": merge_report.unchanged.len(),
            },
            "cas": {
                "root": cas_root.display().to_string(),
                "index_path": index_path.display().to_string(),
            }
        });
        writeln!(writer, "{}", serde_json::to_string_pretty(&summary)?)?;
    } else {
        writeln!(
            writer,
            "{} {} → coords: {}",
            match outcome.blob.status {
                BlobStatus::Stored => "Stored",
                BlobStatus::Reused => "Reused",
            },
            outcome.artifact.describe(),
            coords
        )?;
        writeln!(
            writer,
            "  raw: {} bytes | compressed: {} bytes | digest: {}",
            outcome.blob.raw_size, outcome.blob.compressed_size, outcome.blob.compressed_sha256
        )?;
        writeln!(
            writer,
            "  index path: {} | platform: {} | status: {}",
            outcome.index_entry.path, platform, index_action
        )?;
        writeln!(
            writer,
            "  CAS root: {} | index: {}",
            cas_root.display(),
            index_path.display()
        )?;
    }
    Ok(())
}

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn to_compressed_hash(entry: &CompressedEntry) -> CompressedHash {
    CompressedHash {
        alg: entry.alg.clone(),
        sha256: entry
            .digest
            .as_ref()
            .expect("compressed hash digest missing after normalization")
            .clone(),
    }
}

fn infer_default_path(root: &Path, manifest: Option<&Manifest>, fallback: &str) -> Result<PathBuf> {
    if let Some(manifest) = manifest {
        if let Some(cas) = &manifest.x_cas {
            let candidate = if fallback.ends_with("index.json") {
                &cas.index
            } else {
                &cas.blobs
            };
            let rel = Path::new(candidate);
            package::ensure_safe_relative_path(rel)?;
            return Ok(resolve_path(root, rel));
        }
    }

    let rel = Path::new(fallback);
    package::ensure_safe_relative_path(rel)?;
    let resolved = resolve_path(root, rel);
    if !resolved.exists() {
        return Err(anyhow!(
            "failed to resolve {} – specify --index/--blobs-dir or add manifest.x-cas",
            resolved.display()
        ));
    }
    Ok(resolved)
}

fn resolve_root(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir().context("failed to resolve current directory")?;
        Ok(cwd.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{
        CasConfig, Manifest, NodeInstallOptions, PackageDependencies, PythonInstallOptions,
    };
    use crate::signing::StoredKey;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;
    use zip::write::FileOptions;
    use zip::ZipWriter;
    use zstd::stream::encode_all;

    fn write_zstd_fixture(root: &Path, relative_path: &str, raw: &[u8]) -> (String, String) {
        let blob_root = root.join("cas/blobs");
        let blob_path = blob_root.join(relative_path);
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let compressed = encode_all(raw, 0).unwrap();
        fs::write(&blob_path, &compressed).unwrap();

        let mut raw_hasher = Sha256::new();
        raw_hasher.update(raw);
        let raw_sha = hex::encode(raw_hasher.finalize());

        let mut compressed_hasher = Sha256::new();
        compressed_hasher.update(&compressed);
        let compressed_sha = hex::encode(compressed_hasher.finalize());

        (raw_sha, compressed_sha)
    }

    fn write_index(root: &Path, entries: &[serde_json::Value]) {
        let index_dir = root.join("cas");
        fs::create_dir_all(index_dir.join("blobs")).unwrap();
        let json = serde_json::to_string_pretty(entries).unwrap();
        fs::write(index_dir.join("index.json"), format!("{json}\n")).unwrap();
    }

    fn write_wheel_fixture(
        target: &Path,
        distribution: &str,
        version: &str,
        summary: &str,
        requires_dist: &[&str],
    ) {
        let file = File::create(target).unwrap();
        let mut writer = ZipWriter::new(file);
        let options = FileOptions::default();
        let dist_info_dir = format!("{distribution}-{version}.dist-info/");
        writer
            .add_directory(&dist_info_dir, Default::default())
            .unwrap();
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
        writer
            .start_file(format!("{dist_info_dir}METADATA"), options)
            .unwrap();
        writer.write_all(metadata.as_bytes()).unwrap();
        writer
            .start_file(format!("{dist_info_dir}WHEEL"), options)
            .unwrap();
        writer
            .write_all(
                b"Wheel-Version: 1.0\nGenerator: adep-tests\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
            )
            .unwrap();
        let package_dir = format!("{distribution}/");
        writer
            .add_directory(&package_dir, Default::default())
            .unwrap();
        writer
            .start_file(format!("{distribution}/__init__.py"), options)
            .unwrap();
        writer.write_all(b"__all__ = []\n").unwrap();
        writer.finish().unwrap();
    }

    fn write_pnpm_tarball_fixture(
        target: &Path,
        package_name: &str,
        version: &str,
        description: Option<&str>,
        license: Option<&str>,
        dependencies: &[(&str, &str)],
    ) {
        let mut package = serde_json::Map::new();
        package.insert(
            "name".into(),
            serde_json::Value::String(package_name.to_string()),
        );
        package.insert(
            "version".into(),
            serde_json::Value::String(version.to_string()),
        );
        if let Some(desc) = description {
            package.insert(
                "description".into(),
                serde_json::Value::String(desc.to_string()),
            );
        }
        if let Some(lic) = license {
            package.insert("license".into(), serde_json::Value::String(lic.to_string()));
        }
        if !dependencies.is_empty() {
            let mut deps = serde_json::Map::new();
            for (dep, ver) in dependencies {
                deps.insert(
                    (*dep).to_string(),
                    serde_json::Value::String((*ver).to_string()),
                );
            }
            package.insert("dependencies".into(), serde_json::Value::Object(deps));
        }
        let json_bytes = serde_json::to_vec_pretty(&serde_json::Value::Object(package)).unwrap();

        let tar_bytes = build_tar_archive(&[("package/package.json", json_bytes.as_slice())]);
        let file = File::create(target).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap();
    }

    fn build_tar_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = Vec::new();
        for (path, data) in entries {
            let mut header = [0u8; 512];
            write_tar_header(&mut header, path, data.len());
            archive.extend_from_slice(&header);
            archive.extend_from_slice(data);
            let padding = (512 - (data.len() % 512)) % 512;
            archive.extend(std::iter::repeat(0u8).take(padding));
        }
        archive.extend_from_slice(&[0u8; 1024]);
        archive
    }

    fn write_tar_header(header: &mut [u8; 512], path: &str, size: usize) {
        header.fill(0);
        let name_bytes = path.as_bytes();
        let len = name_bytes.len().min(100);
        header[..len].copy_from_slice(&name_bytes[..len]);
        write_tar_octal_field(&mut header[100..108], 0o644);
        write_tar_octal_field(&mut header[108..116], 0);
        write_tar_octal_field(&mut header[116..124], 0);
        write_tar_octal_field(&mut header[124..136], size as u64);
        write_tar_octal_field(&mut header[136..148], 0);
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[148..156].fill(b' ');
        let checksum: u32 = header.iter().map(|&b| b as u32).sum();
        write_tar_octal_field(&mut header[148..156], checksum as u64);
    }

    fn write_tar_octal_field(field: &mut [u8], value: u64) {
        if field.is_empty() {
            return;
        }
        let width = field.len().saturating_sub(1);
        let formatted = if width == 0 {
            String::new()
        } else {
            format!("{value:0width$o}\0", width = width)
        };
        let bytes = formatted.as_bytes();
        let copy_len = bytes.len().min(field.len());
        field[..copy_len].copy_from_slice(&bytes[..copy_len]);
        if copy_len < field.len() {
            for byte in &mut field[copy_len..] {
                *byte = 0;
            }
        }
        field[field.len() - 1] = b' ';
    }

    fn write_executable_script(path: &Path, content: &str) {
        fs::write(path, content).unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn deps_verify_success() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let blobs_dir = root.join("cas/blobs/dist");
        fs::create_dir_all(&blobs_dir).unwrap();
        let blob_path = blobs_dir.join("blob.bin");
        fs::write(&blob_path, b"hello").unwrap();
        let mut hasher = Sha256::new();
        hasher.update(b"hello");
        let sha = hex::encode(hasher.finalize());
        write_index(
            root,
            &[serde_json::json!({
                "path": "dist/blob.bin",
                "raw_sha256": sha,
                "size": 5
            })],
        );
        let mut manifest = Manifest::template(None, None);
        manifest.x_cas = Some(CasConfig {
            index: "cas/index.json".into(),
            blobs: "cas/blobs".into(),
            policy: None,
        });
        manifest.save(&root.join("manifest.json")).unwrap();

        let args = VerifyArgs {
            root: root.to_path_buf(),
            index: None,
            blobs_dir: None,
            json: false,
        };

        run_verify(&args).expect("verification should succeed");
    }

    #[test]
    fn deps_verify_zstd_success() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let raw = b"compressed payload bytes";
        let rel_path = "dist/asset.bin";
        let (raw_sha, compressed_sha) = write_zstd_fixture(root, rel_path, raw);

        write_index(
            root,
            &[serde_json::json!({
                "path": rel_path,
                "raw_sha256": raw_sha,
                "size": raw.len(),
                "compressed": {
                    "alg": "zstd"
                },
                "compressed_sha256": compressed_sha
            })],
        );

        let mut manifest = Manifest::template(None, None);
        manifest.x_cas = Some(CasConfig {
            index: "cas/index.json".into(),
            blobs: "cas/blobs".into(),
            policy: None,
        });
        manifest.save(&root.join("manifest.json")).unwrap();

        let args = VerifyArgs {
            root: root.to_path_buf(),
            index: None,
            blobs_dir: None,
            json: false,
        };

        run_verify(&args).expect("zstd verification should succeed");
    }

    #[test]
    fn deps_verify_rejects_path_traversal() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        write_index(
            root,
            &[serde_json::json!({
                "path": "../escape.bin",
                "raw_sha256": "deadbeef",
            })],
        );
        let mut manifest = Manifest::template(None, None);
        manifest.x_cas = Some(CasConfig {
            index: "cas/index.json".into(),
            blobs: "cas/blobs".into(),
            policy: None,
        });
        manifest.save(&root.join("manifest.json")).unwrap();

        let args = VerifyArgs {
            root: root.to_path_buf(),
            index: None,
            blobs_dir: None,
            json: false,
        };

        let err = run_verify(&args).expect_err("should fail due to traversal");
        assert!(
            err.to_string().contains("outside CAS root"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn deps_verify_unsupported_compression() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        write_index(
            root,
            &[serde_json::json!({
                "path": "dist/blob.bin",
                "raw_sha256": "deadbeef",
                "compressed": {
                    "alg": "brotli",
                    "sha256": "beadfeed"
                },
                "compressed_sha256": "beadfeed"
            })],
        );
        let mut manifest = Manifest::template(None, None);
        manifest.x_cas = Some(CasConfig {
            index: "cas/index.json".into(),
            blobs: "cas/blobs".into(),
            policy: None,
        });
        manifest.save(&root.join("manifest.json")).unwrap();

        let args = VerifyArgs {
            root: root.to_path_buf(),
            index: None,
            blobs_dir: None,
            json: false,
        };
        let err = run_verify(&args).expect_err("compressed entry should fail");
        let has_keyword = err.chain().any(|cause| {
            cause
                .to_string()
                .to_lowercase()
                .contains("unsupported compression")
        });
        assert!(has_keyword, "unexpected error: {err}");
    }

    #[test]
    fn deps_verify_zstd_compressed_hash_mismatch_error() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let raw = b"compressed entry mismatch";
        let rel_path = "dist/asset.bin";
        let (raw_sha, _) = write_zstd_fixture(root, rel_path, raw);

        write_index(
            root,
            &[serde_json::json!({
                "path": rel_path,
                "raw_sha256": raw_sha,
                "size": raw.len(),
                "compressed": {
                    "alg": "zstd",
                    "sha256": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                },
                "compressed_sha256": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            })],
        );

        let mut manifest = Manifest::template(None, None);
        manifest.x_cas = Some(CasConfig {
            index: "cas/index.json".into(),
            blobs: "cas/blobs".into(),
            policy: None,
        });
        manifest.save(&root.join("manifest.json")).unwrap();

        let args = VerifyArgs {
            root: root.to_path_buf(),
            index: None,
            blobs_dir: None,
            json: false,
        };

        let err = run_verify(&args).expect_err("compressed hash mismatch should fail");
        assert!(
            err.to_string()
                .to_lowercase()
                .contains("compressed hash mismatch"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn deps_verify_rejects_duplicate_coords() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        write_index(
            root,
            &[
                serde_json::json!({
                    "path": "dist/pkg1.whl",
                    "coords": ["pkg:pypi/numpy@1.0.0"],
                    "raw_sha256": "raw1",
                    "compressed": { "alg": "zstd" },
                    "compressed_sha256": "comp1"
                }),
                serde_json::json!({
                    "path": "dist/pkg2.whl",
                    "coords": ["pkg:pypi/numpy@1.0.0"],
                    "raw_sha256": "raw2",
                    "compressed": { "alg": "zstd" },
                    "compressed_sha256": "comp2"
                }),
            ],
        );
        Manifest::template(None, None)
            .save(&root.join("manifest.json"))
            .unwrap();
        let args = VerifyArgs {
            root: root.to_path_buf(),
            index: None,
            blobs_dir: None,
            json: false,
        };
        let err = run_verify(&args).expect_err("duplicate coords should fail");
        assert!(err.to_string().contains("coord 'pkg:pypi/numpy@1.0.0'"));
    }

    #[test]
    fn deps_vendor_python_wheel_ingests_blob() {
        let temp = TempDir::new().unwrap();
        let cas_root = temp.path().join("cas");
        let wheel_path = temp.path().join("example_pkg-1.0.0-py3-none-any.whl");
        write_wheel_fixture(
            &wheel_path,
            "example_pkg",
            "1.0.0",
            "Example fixture package",
            &["requests (>=2.0)"],
        );

        let args = VendorArgs {
            lang: VendorLanguage::Python,
            source: wheel_path.clone(),
            coords: None,
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: true,
        };

        run_vendor(&args).expect("vendor should ingest wheel");

        let index_path = cas_root.join("index.json");
        let index_file = File::open(&index_path).unwrap();
        let index = CanonicalIndex::from_reader(index_file).unwrap();
        assert_eq!(index.entries().len(), 1);
        let entry = &index.entries()[0];
        assert!(entry.path.starts_with("sha256-"));
        assert_eq!(entry.coords, vec!["pkg:pypi/example-pkg@1.0.0".to_string()]);
        assert_eq!(entry.platform, vec!["any".to_string()]);
        assert!(entry.compressed_sha256.is_some());
        assert!(entry.size.is_some());
        let metadata = entry.metadata.as_ref().expect("metadata present");
        assert_eq!(
            metadata.filename.as_deref(),
            Some("example_pkg-1.0.0-py3-none-any.whl")
        );
        assert_eq!(metadata.kind.as_deref(), Some("python-wheel"));
    }

    #[test]
    fn deps_vendor_reuses_existing_blob() {
        let temp = TempDir::new().unwrap();
        let cas_root = temp.path().join("cas");
        let wheel_path = temp.path().join("reuse_pkg-2.0.0-py3-none-any.whl");
        write_wheel_fixture(&wheel_path, "reuse_pkg", "2.0.0", "Reusable wheel", &[]);

        let base_args = VendorArgs {
            lang: VendorLanguage::Python,
            source: wheel_path.clone(),
            coords: None,
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: false,
        };

        run_vendor(&base_args).expect("first ingestion should succeed");
        run_vendor(&base_args).expect("second ingestion should reuse");

        let index_path = cas_root.join("index.json");
        let index_file = File::open(&index_path).unwrap();
        let index = CanonicalIndex::from_reader(index_file).unwrap();
        assert_eq!(index.entries().len(), 1);
    }

    #[test]
    fn deps_vendor_conflicting_coords_error() {
        let temp = TempDir::new().unwrap();
        let cas_root = temp.path().join("cas");
        let wheel_path = temp.path().join("conflict_pkg-1.0.0-py3-none-any.whl");
        write_wheel_fixture(&wheel_path, "conflict_pkg", "1.0.0", "Initial release", &[]);

        let args = VendorArgs {
            lang: VendorLanguage::Python,
            source: wheel_path.clone(),
            coords: None,
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: false,
        };

        run_vendor(&args).expect("initial ingestion should succeed");

        write_wheel_fixture(
            &wheel_path,
            "conflict_pkg",
            "1.0.0",
            "Modified release",
            &["depA (>=1.0)"],
        );
        let err = run_vendor(&args).expect_err("conflicting coords should fail");
        assert!(err.to_string().contains("coord"));

        // Ensure index still has single entry
        let index_path = cas_root.join("index.json");
        let index_file = File::open(&index_path).unwrap();
        let index = CanonicalIndex::from_reader(index_file).unwrap();
        assert_eq!(index.entries().len(), 1);
    }

    #[test]
    fn deps_vendor_python_summary_json_contains_metadata() {
        let temp = TempDir::new().unwrap();
        let cas_root = temp.path().join("cas");
        let wheel_path = temp.path().join("meta_pkg-3.2.1-py3-none-any.whl");
        write_wheel_fixture(
            &wheel_path,
            "meta_pkg",
            "3.2.1",
            "Metadata rich fixture",
            &["depB (==2.0)"],
        );

        let args = VendorArgs {
            lang: VendorLanguage::Python,
            source: wheel_path.clone(),
            coords: None,
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: true,
        };

        let store = BlobStore::open(&cas_root).unwrap();
        let outcome = ingest_python_wheel(&store, &wheel_path, &args).unwrap();
        if let ArtifactMetadata::PythonWheel(meta) = &outcome.artifact {
            assert_eq!(meta.metadata_name, "meta-pkg");
            assert_eq!(meta.summary.as_deref(), Some("Metadata rich fixture"));
            assert_eq!(meta.requires_python.as_deref(), Some(">=3.9"));
            assert_eq!(meta.requires_dist, vec!["depB (==2.0)".to_string()]);
        } else {
            panic!("expected python metadata");
        }

        let current_index = CanonicalIndex::new(Vec::new());
        let merge_report = current_index
            .merge(vec![outcome.index_entry.clone()])
            .unwrap();

        let mut buffer = Vec::new();
        emit_vendor_summary(
            &args,
            &cas_root,
            &cas_root.join("index.json"),
            &outcome,
            &merge_report,
            &mut buffer,
        )
        .unwrap();
        let output = String::from_utf8(buffer).unwrap();
        let summary_json: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            summary_json["artifact"]["summary"],
            serde_json::Value::String("Metadata rich fixture".into())
        );
        assert_eq!(
            summary_json["artifact"]["requires_python"],
            serde_json::Value::String(">=3.9".into())
        );
        assert_eq!(
            summary_json["artifact"]["requires_dist"],
            serde_json::json!(["depB (==2.0)"])
        );
    }

    #[test]
    fn deps_vendor_python_coords_validation_error() {
        let temp = TempDir::new().unwrap();
        let cas_root = temp.path().join("cas");
        let wheel_path = temp.path().join("coords_pkg-1.2.3-py3-none-any.whl");
        write_wheel_fixture(&wheel_path, "coords_pkg", "1.2.3", "Coords case", &[]);

        let args = VendorArgs {
            lang: VendorLanguage::Python,
            source: wheel_path.clone(),
            coords: Some("pkg:pypi/other@1.2.3".into()),
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: false,
        };

        let store = BlobStore::open(&cas_root).unwrap();
        let err = ingest_python_wheel(&store, &wheel_path, &args)
            .expect_err("mismatched coords should fail");
        assert!(err.to_string().contains("coords name"));
    }

    #[test]
    fn deps_vendor_pnpm_tarball_ingests_metadata() {
        let temp = TempDir::new().unwrap();
        let cas_root = temp.path().join("cas");
        let tar_path = temp.path().join("left-pad-1.0.0.tgz");
        write_pnpm_tarball_fixture(
            &tar_path,
            "left-pad",
            "1.0.0",
            Some("Pads strings on the left"),
            Some("MIT"),
            &[("lodash", "^4.0.0")],
        );

        let args = VendorArgs {
            lang: VendorLanguage::Node,
            source: tar_path.clone(),
            coords: None,
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: true,
        };

        let store = BlobStore::open(&cas_root).unwrap();
        let outcome = ingest_pnpm_tarball(&store, &tar_path, &args).unwrap();
        if let ArtifactMetadata::PnpmTarball(meta) = &outcome.artifact {
            assert_eq!(meta.package_name, "left-pad");
            assert_eq!(meta.version, "1.0.0");
            assert_eq!(
                meta.description.as_deref(),
                Some("Pads strings on the left")
            );
            assert_eq!(meta.license.as_deref(), Some("MIT"));
            assert_eq!(meta.dependencies.get("lodash"), Some(&"^4.0.0".to_string()));
        } else {
            panic!("expected pnpm metadata");
        }
        let current_index = CanonicalIndex::new(Vec::new());
        let merge_report = current_index
            .merge(vec![outcome.index_entry.clone()])
            .unwrap();
        let mut buffer = Vec::new();
        emit_vendor_summary(
            &args,
            &cas_root,
            &cas_root.join("index.json"),
            &outcome,
            &merge_report,
            &mut buffer,
        )
        .unwrap();
        let summary_json: serde_json::Value =
            serde_json::from_str(&String::from_utf8(buffer).unwrap()).unwrap();
        assert_eq!(
            summary_json["artifact"]["description"],
            "Pads strings on the left"
        );
        assert_eq!(summary_json["artifact"]["dependencies"]["lodash"], "^4.0.0");

        run_vendor(&args).expect("vendor should ingest tarball");
        let index_path = cas_root.join("index.json");
        let index_file = File::open(&index_path).unwrap();
        let index = CanonicalIndex::from_reader(index_file).unwrap();
        assert_eq!(index.entries().len(), 1);
        assert_eq!(
            index.entries()[0].coords,
            vec!["pkg:npm/left-pad@1.0.0".to_string()]
        );
    }

    #[test]
    fn deps_vendor_pnpm_coords_validation_error() {
        let temp = TempDir::new().unwrap();
        let cas_root = temp.path().join("cas");
        let tar_path = temp.path().join("left-pad-1.0.0.tgz");
        write_pnpm_tarball_fixture(&tar_path, "left-pad", "1.0.0", None, None, &[]);

        let args = VendorArgs {
            lang: VendorLanguage::Node,
            source: tar_path.clone(),
            coords: Some("pkg:npm/other@1.0.0".into()),
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: false,
        };

        let store = BlobStore::open(&cas_root).unwrap();
        let err = ingest_pnpm_tarball(&store, &tar_path, &args)
            .expect_err("mismatched coords should fail");
        assert!(err.to_string().contains("coords name"));
    }

    #[test]
    fn deps_resolve_and_install_offline_success() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let cas_root = root.join("cas");
        let wheel_path = root.join("example_pkg-1.0.0-py3-none-any.whl");
        write_wheel_fixture(
            &wheel_path,
            "example_pkg",
            "1.0.0",
            "Example fixture package",
            &["requests (>=2.0)"],
        );
        let tar_path = root.join("left-pad-1.0.0.tgz");
        write_pnpm_tarball_fixture(
            &tar_path,
            "left-pad",
            "1.0.0",
            Some("Pad to the left"),
            Some("MIT"),
            &[],
        );

        let mut manifest = Manifest::template(Some("1.0.0".into()), None);
        manifest.deps = Some(PackageDependencies {
            python: Some(PythonDependencies {
                requirements: "requirements.lock".into(),
                source: Some("cas://pypi/wheels".into()),
                install: Some(PythonInstallOptions {
                    mode: "offline".into(),
                    target: Some(".venv".into()),
                    no_deps: Some(true),
                }),
            }),
            node: Some(NodeDependencies {
                lockfile: "pnpm-lock.yaml".into(),
                store: Some("cas://pnpm".into()),
                install: Some(NodeInstallOptions {
                    mode: "offline".into(),
                    frozen_lockfile: Some(true),
                }),
            }),
        });
        manifest.save(&root.join("manifest.json")).unwrap();
        fs::write(
            root.join("requirements.lock"),
            "example-pkg==1.0.0 --hash=sha256:deadbeef\n",
        )
        .unwrap();
        fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: 5.4\n").unwrap();

        let python_vendor = VendorArgs {
            lang: VendorLanguage::Python,
            source: wheel_path.clone(),
            coords: None,
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: false,
        };
        run_vendor(&python_vendor).expect("python vendor should succeed");

        let node_vendor = VendorArgs {
            lang: VendorLanguage::Node,
            source: tar_path.clone(),
            coords: None,
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: false,
        };
        run_vendor(&node_vendor).expect("node vendor should succeed");

        let key_path = root.join("deps-key.json");
        StoredKey::generate().write(&key_path).unwrap();

        let capsule_args = CapsuleArgs {
            root: root.to_path_buf(),
            manifest: None,
            index: None,
            blobs_dir: None,
            output: None,
            key: Some(key_path),
            json: false,
        };
        run_capsule(&capsule_args).expect("capsule generation should succeed");

        let bin_dir = root.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let log_dir = root.join("logs");
        fs::create_dir_all(&log_dir).unwrap();
        let pip_log = log_dir.join("pip.log");
        let pnpm_log = log_dir.join("pnpm.log");
        let pip_log_str = pip_log.display().to_string().replace('"', "\\\"");
        let pnpm_log_str = pnpm_log.display().to_string().replace('"', "\\\"");

        let pip_script = format!(
            r#"#!/bin/sh
echo "$@" >> "{log}"
for arg in "$@"; do
  case "$arg" in
    --require-hashes) hash=1 ;;
    --no-deps) nodeps=1 ;;
    --no-index) noindex=1 ;;
  esac
done
if [ -z "$hash" ] || [ -z "$nodeps" ] || [ -z "$noindex" ]; then
  echo "missing flags" >&2
  exit 21
fi
exit 0
"#,
            log = pip_log_str
        );
        write_executable_script(&bin_dir.join("pip"), &pip_script);

        let pnpm_script = format!(
            r#"#!/bin/sh
echo "$@" >> "{log}"
if [ "$1" != "install" ]; then
  echo "unexpected subcommand" >&2
  exit 22
fi
offline=0
frozen=0
store=0
for arg in "$@"; do
  case "$arg" in
    --offline) offline=1 ;;
    --frozen-lockfile) frozen=1 ;;
    --store-dir) store=1 ;;
  esac
done
if [ $offline -eq 0 ] || [ $frozen -eq 0 ] || [ $store -eq 0 ]; then
  echo "missing flags" >&2
  exit 23
fi
exit 0
"#,
            log = pnpm_log_str
        );
        write_executable_script(&bin_dir.join("pnpm"), &pnpm_script);

        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", bin_dir.display(), original_path);
        std::env::set_var("PATH", &new_path);

        let resolve_args = ResolveArgs {
            root: root.to_path_buf(),
            manifest: None,
            capsule: None,
            cas_dir: Some(cas_root.clone()),
            output: None,
            json: false,
        };
        run_resolve(&resolve_args).expect("resolve should succeed");

        let wheels_dir = root.join("deps-cache/python/wheels");
        assert!(wheels_dir
            .join("example_pkg-1.0.0-py3-none-any.whl")
            .exists());
        let store_dir = root.join("deps-cache/node/store");
        assert!(store_dir.join("left-pad-1.0.0.tgz").exists());

        let install_args = InstallArgs {
            root: root.to_path_buf(),
            manifest: None,
            capsule: None,
            cas_dir: Some(cas_root.clone()),
            output: None,
            pip: None,
            pnpm: None,
            dry_run: false,
            json: false,
        };
        run_install(&install_args).expect("install should succeed");

        let pip_log_content = fs::read_to_string(&pip_log).unwrap();
        assert!(pip_log_content.contains("--require-hashes"));
        assert!(pip_log_content.contains("--no-deps"));
        assert!(pip_log_content.contains("--no-index"));
        let pnpm_log_content = fs::read_to_string(&pnpm_log).unwrap();
        assert!(pnpm_log_content.contains("--offline"));
        assert!(pnpm_log_content.contains("--frozen-lockfile"));
        assert!(pnpm_log_content.contains("--store-dir"));

        std::env::set_var("PATH", original_path);
    }

    #[test]
    fn deps_resolve_missing_blob_fails() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let cas_root = root.join("cas");
        let wheel_path = root.join("missing_pkg-1.0.0-py3-none-any.whl");
        write_wheel_fixture(&wheel_path, "missing_pkg", "1.0.0", "Missing blob", &[]);

        let mut manifest = Manifest::template(Some("1.0.0".into()), None);
        manifest.deps = Some(PackageDependencies {
            python: Some(PythonDependencies {
                requirements: "requirements.lock".into(),
                source: Some("cas://pypi/wheels".into()),
                install: Some(PythonInstallOptions {
                    mode: "offline".into(),
                    target: None,
                    no_deps: Some(true),
                }),
            }),
            node: None,
        });
        manifest.save(&root.join("manifest.json")).unwrap();
        fs::write(
            root.join("requirements.lock"),
            "missing-pkg==1.0.0 --hash=sha256:deadbeef\n",
        )
        .unwrap();

        let vendor_args = VendorArgs {
            lang: VendorLanguage::Python,
            source: wheel_path.clone(),
            coords: None,
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: false,
        };
        run_vendor(&vendor_args).expect("vendor should succeed");

        let key_path = root.join("deps-key.json");
        StoredKey::generate().write(&key_path).unwrap();
        let capsule_args = CapsuleArgs {
            root: root.to_path_buf(),
            manifest: None,
            index: None,
            blobs_dir: None,
            output: None,
            key: Some(key_path),
            json: false,
        };
        run_capsule(&capsule_args).expect("capsule generation should succeed");

        let capsule_path = root.join("cas").join("capsule-manifest.json");
        let capsule = super::load_capsule_manifest(&capsule_path).unwrap();
        let entry = capsule.entries().first().expect("entry present");
        let blob_path = cas_root.join("blobs").join(&entry.path);
        assert!(blob_path.exists());
        fs::remove_file(&blob_path).unwrap();

        let resolve_args = ResolveArgs {
            root: root.to_path_buf(),
            manifest: None,
            capsule: None,
            cas_dir: Some(cas_root.clone()),
            output: None,
            json: false,
        };
        let err = run_resolve(&resolve_args).expect_err("resolve should fail");
        assert!(
            err.to_string().contains("missing"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn deps_push_pull_oci_layout_roundtrip() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let cas_root = root.join("cas");
        let wheel_path = root.join("roundtrip_pkg-1.0.0-py3-none-any.whl");
        write_wheel_fixture(
            &wheel_path,
            "roundtrip_pkg",
            "1.0.0",
            "Roundtrip fixture",
            &[],
        );

        Manifest::template(Some("1.0.0".into()), None)
            .save(&root.join("manifest.json"))
            .unwrap();

        let vendor_args = VendorArgs {
            lang: VendorLanguage::Python,
            source: wheel_path.clone(),
            coords: None,
            platform: None,
            cas_dir: Some(cas_root.clone()),
            json: false,
        };
        run_vendor(&vendor_args).expect("vendor should succeed");

        let key_path = root.join("deps-key.json");
        StoredKey::generate().write(&key_path).unwrap();
        let capsule_args = CapsuleArgs {
            root: root.to_path_buf(),
            manifest: None,
            index: None,
            blobs_dir: None,
            output: None,
            key: Some(key_path),
            json: false,
        };
        run_capsule(&capsule_args).expect("capsule generation should succeed");

        let capsule_path = root.join("cas").join("capsule-manifest.json");
        let capsule = super::load_capsule_manifest(&capsule_path).unwrap();
        let entry = capsule.entries().first().expect("capsule entry");
        let entry_path = entry.path.clone();

        let registry_dir = root.join("registry");
        let push_args = PushArgs {
            root: root.to_path_buf(),
            capsule: None,
            registry: registry_dir.display().to_string(),
            reference: "local/deps:1.0.0".into(),
            cas_dir: Some(cas_root.clone()),
            json: false,
        };
        run_push(&push_args).expect("push should succeed");

        let index_json: serde_json::Value =
            serde_json::from_slice(&fs::read(registry_dir.join("index.json")).unwrap()).unwrap();
        assert!(index_json["manifests"]
            .as_array()
            .unwrap()
            .iter()
            .any(
                |entry| entry["annotations"]["org.opencontainers.image.ref.name"]
                    == "local/deps:1.0.0"
            ));

        let pull_cas = root.join("cas-pull");
        let pull_args = PullArgs {
            cas_dir: Some(pull_cas.clone()),
            output: None,
            registry: registry_dir.display().to_string(),
            reference: "local/deps:1.0.0".into(),
            json: false,
        };
        run_pull(&pull_args).expect("pull should succeed");

        let pulled_blob = pull_cas.join("blobs").join(&entry_path);
        assert!(
            pulled_blob.exists(),
            "expected pulled blob at {}",
            pulled_blob.display()
        );
        let pulled_capsule = pull_cas.join("capsule-manifest.json");
        assert!(
            pulled_capsule.exists(),
            "expected capsule manifest at {}",
            pulled_capsule.display()
        );
    }

    #[test]
    fn deps_verify_defaults_without_xcas() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let blobs_dir = root.join("cas/blobs");
        fs::create_dir_all(&blobs_dir).unwrap();
        let blob_path = blobs_dir.join("foo.bin");
        fs::write(&blob_path, b"default").unwrap();
        let mut hasher = Sha256::new();
        hasher.update(b"default");
        let sha = hex::encode(hasher.finalize());
        write_index(
            root,
            &[serde_json::json!({
                "path": "foo.bin",
                "raw_sha256": sha,
                "size": 7
            })],
        );
        Manifest::template(None, None)
            .save(&root.join("manifest.json"))
            .unwrap();

        let args = VerifyArgs {
            root: root.to_path_buf(),
            index: None,
            blobs_dir: None,
            json: false,
        };

        run_verify(&args).expect("defaults should verify");
    }

    #[test]
    fn deps_verify_defaults_missing_paths() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        Manifest::template(None, None)
            .save(&root.join("manifest.json"))
            .unwrap();

        let args = VerifyArgs {
            root: root.to_path_buf(),
            index: None,
            blobs_dir: None,
            json: false,
        };
        let err = run_verify(&args).expect_err("should fail because default paths missing");
        assert!(err.to_string().contains("specify --index/--blobs-dir"));
    }
}
