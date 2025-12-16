use crate::deps::{apply_dependency_defaults, DependencyDefaults};
use alloc::{string::String, vec::Vec};
use std::{
    collections::BTreeMap,
    fmt,
    fs::File,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::de::{self, Deserializer, Visitor};
use serde::ser::{SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Manifest {
    #[serde(rename = "schemaVersion", default = "Manifest::default_schema_version")]
    pub schema_version: String,
    pub id: Uuid,
    pub family_id: Uuid,
    pub version: VersionInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub developer_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_rotation: Option<KeyRotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publish_info: Option<PublishInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<PlatformSpec>,
    #[serde(default)]
    pub network: NetworkPolicy,
    #[serde(default, skip_serializing_if = "PackSpec::is_empty")]
    pub pack: PackSpec,
    #[serde(rename = "x-cas", default, skip_serializing_if = "Option::is_none")]
    pub x_cas: Option<CasConfig>,
    #[serde(
        rename = "dep_capsules",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub dep_capsules: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deps: Option<PackageDependencies>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceLimits>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub files: Vec<FileEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Dependencies>,
}

impl Manifest {
    fn default_schema_version() -> String {
        "1.2".to_string()
    }

    pub fn template(version: Option<String>, channel: Option<String>) -> Self {
        Self {
            schema_version: Self::default_schema_version(),
            id: Uuid::new_v4(),
            family_id: Uuid::new_v4(),
            version: VersionInfo {
                number: version.unwrap_or_else(|| "0.1.0".to_string()),
                channel: channel.unwrap_or_else(|| "stable".to_string()),
                commit: None,
                label: None,
            },
            developer_key: None,
            key_rotation: None,
            publish_info: None,
            runtime: None,
            platform: None,
            network: NetworkPolicy::default(),
            pack: PackSpec::with_profile("dist+cas"),
            x_cas: None,
            dep_capsules: Vec::new(),
            deps: None,
            resources: None,
            capabilities: Vec::new(),
            files: Vec::new(),
            dependencies: None,
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let file =
            File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut manifest: Manifest = serde_json::from_reader(reader)
            .with_context(|| format!("failed to parse manifest {}", path.display()))?;
        manifest.files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(manifest)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let file =
            File::create(path).with_context(|| format!("failed to write {}", path.display()))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)
            .with_context(|| format!("failed to serialize manifest {}", path.display()))?;
        Ok(())
    }
}

/// Options controlling how defaults are applied to a manifest.
#[derive(Clone)]
pub struct DefaultOptions<'a> {
    /// Default `pack.profile` to enforce when missing.
    pub default_pack_profile: &'a str,
    /// Optional dependency defaults resolver.
    pub dependency_defaults: Option<&'a dyn DependencyDefaults>,
}

impl Default for DefaultOptions<'_> {
    fn default() -> Self {
        Self {
            default_pack_profile: "dist+cas",
            dependency_defaults: None,
        }
    }
}

impl<'a> DefaultOptions<'a> {
    pub fn with_dependency_defaults(mut self, resolver: &'a dyn DependencyDefaults) -> Self {
        self.dependency_defaults = Some(resolver);
        self
    }
}

/// Report describing the results of applying defaults to a manifest.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub warnings: Vec<ManifestWarning>,
    pub mutated: bool,
}

/// A warning emitted while normalizing or defaulting manifest values.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ManifestWarning {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl ManifestWarning {
    pub fn new<S: Into<String>, M: Into<String>>(code: S, message: M) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: None,
        }
    }

    pub fn with_path<S: Into<String>, M: Into<String>, P: Into<String>>(
        code: S,
        message: M,
        path: P,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: Some(path.into()),
        }
    }
}

const WARN_PACK_PROFILE_DEFAULTED: &str = "PACK-PROFILE-DEFAULTED";
const WARN_PACK_PROFILE_MISMATCH: &str = "PACK-PROFILE-MISMATCH";

/// Apply canonical defaults to the manifest structure and capture any warnings.
pub fn apply_defaults(manifest: &mut Manifest, opts: DefaultOptions<'_>) -> ApplyReport {
    let mut report = ApplyReport::default();

    match manifest.pack.profile.as_deref() {
        Some(profile) if profile == opts.default_pack_profile => {}
        Some(profile) => {
            report.warnings.push(ManifestWarning::with_path(
                WARN_PACK_PROFILE_MISMATCH,
                format!(
                    "pack.profile is '{}'; expected '{}' for offline dist+cas packages",
                    profile, opts.default_pack_profile
                ),
                "pack.profile",
            ));
        }
        None => {
            manifest.pack.profile = Some(opts.default_pack_profile.to_string());
            report.mutated = true;
            report.warnings.push(ManifestWarning::with_path(
                WARN_PACK_PROFILE_DEFAULTED,
                format!(
                    "pack.profile missing; defaulted to {}",
                    opts.default_pack_profile
                ),
                "pack.profile",
            ));
        }
    }

    let (dep_warnings, deps_mutated) = apply_dependency_defaults(manifest, &opts);
    if deps_mutated {
        report.mutated = true;
    }
    report.warnings.extend(dep_warnings);

    report
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct PackSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub engines: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pm: Option<String>,
}

impl PackSpec {
    pub fn is_empty(&self) -> bool {
        self.profile.is_none() && self.engines.is_empty() && self.pm.is_none()
    }

    pub fn with_profile(profile: &str) -> Self {
        Self {
            profile: Some(profile.to_string()),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct VersionInfo {
    pub number: String,
    pub channel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct KeyRotation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PublishInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<WidgetInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WidgetInfo {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RuntimeSpec {
    #[serde(rename = "type")]
    pub runtime_type: String,
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PlatformSpec {
    pub language: String,
    pub version: String,
    pub entry: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wheels: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum EgressMode {
    Dev,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NetworkPolicy {
    #[serde(default)]
    pub egress_allow: Vec<String>,
    #[serde(default)]
    pub egress_id_allow: Vec<EgressIdRule>,
    #[serde(default = "NetworkPolicy::default_http_proxy")]
    pub http_proxy_dev: bool,
    #[serde(default)]
    pub egress_mode: Option<EgressMode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, NetworkProfile>,
    #[serde(rename = "use", default, skip_serializing_if = "Vec::is_empty")]
    pub use_profiles: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_resolver: Option<DevResolverMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CasConfig {
    pub index: String,
    pub blobs: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<CasPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CasPolicy {
    #[serde(rename = "trustDomain")]
    pub trust_domain: String,
    #[serde(default, rename = "verify", skip_serializing_if = "Vec::is_empty")]
    pub verify: Vec<String>,
}

impl NetworkPolicy {
    fn default_http_proxy() -> bool {
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum DevResolverMode {
    SpireLite,
    AdhocPinned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum EgressIdType {
    AdepId,
    AdepFamily,
    Spiffe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum TrustMode {
    Spiffe,
    SpkiPin,
    CfAccess,
    Tailscale,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NetworkProfile {
    #[serde(default)]
    pub egress_allow: Vec<String>,
    #[serde(default)]
    pub egress_id_allow: Vec<EgressIdRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_proxy_dev: Option<bool>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EgressIdRule {
    #[serde(rename = "type")]
    pub rule_type: EgressIdType,
    pub value: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scheme: Vec<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_ports",
        serialize_with = "serialize_ports"
    )]
    pub ports: Vec<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path_prefix: Vec<String>,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub mtls_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<EgressTrust>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EgressTrust {
    pub mode: TrustMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spiffe_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spki_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_token_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailscale_tag: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

fn deserialize_ports<'de, D>(deserializer: D) -> Result<Vec<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    struct PortsVisitor;

    impl<'de> Visitor<'de> for PortsVisitor {
        type Value = Vec<u16>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a port number or array of port numbers")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if value == 0 || value > u16::MAX as u64 {
                return Err(E::custom(format!("port value {} out of range", value)));
            }
            Ok(vec![value as u16])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut ports = Vec::new();
            while let Some(value) = seq.next_element::<u64>()? {
                if value == 0 || value > u16::MAX as u64 {
                    return Err(de::Error::custom(format!(
                        "port value {} out of range",
                        value
                    )));
                }
                ports.push(value as u16);
            }
            Ok(ports)
        }
    }

    deserializer.deserialize_any(PortsVisitor)
}

fn serialize_ports<S>(ports: &Vec<u16>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match ports.len() {
        0 => serializer.serialize_seq(Some(0))?.end(),
        1 => serializer.serialize_u16(ports[0]),
        _ => {
            let mut seq = serializer.serialize_seq(Some(ports.len()))?;
            for port in ports {
                seq.serialize_element(port)?;
            }
            seq.end()
        }
    }
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ResourceLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Dependencies {
    #[serde(default)]
    pub adep: Vec<AdepDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PackageDependencies {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python: Option<PythonDependencies>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeDependencies>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PythonDependencies {
    pub requirements: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<PythonInstallOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PythonInstallOptions {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(rename = "no_deps", default, skip_serializing_if = "Option::is_none")]
    pub no_deps: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeDependencies {
    pub lockfile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<NodeInstallOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct NodeInstallOptions {
    pub mode: String,
    #[serde(
        rename = "frozen_lockfile",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub frozen_lockfile: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AdepDependency {
    pub name: String,
    pub family_id: String,
    #[serde(default)]
    pub port: Option<u16>,
}

pub struct RuntimeProfile<'a> {
    pub runtime: &'a RuntimeSpec,
    pub platform: &'a PlatformSpec,
}

impl Manifest {
    pub fn runtime_profile(&self) -> Option<RuntimeProfile<'_>> {
        match (self.runtime.as_ref(), self.platform.as_ref()) {
            (Some(runtime), Some(platform)) => Some(RuntimeProfile { runtime, platform }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FileEntry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressed: Option<FileCompression>,
}

impl FileEntry {
    pub fn new(path: String, sha256: String, size: u64, role: String) -> Self {
        Self {
            path,
            sha256,
            size,
            role,
            compressed: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FileCompression {
    pub alg: String,
    pub size: u64,
    pub sha256: String,
}

pub fn parse_developer_key(value: &str) -> Result<[u8; 32]> {
    let value = value
        .strip_prefix("ed25519:")
        .ok_or_else(|| anyhow!("developer_key must start with ed25519:"))?;
    let decoded = BASE64
        .decode(value)
        .map_err(|err| anyhow!("failed to decode developer_key: {err}"))?;
    if decoded.len() != 32 {
        bail!(
            "developer_key must decode to 32 bytes, got {}",
            decoded.len()
        );
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&decoded);
    Ok(bytes)
}

/// Validate manifest metadata according to ADEP v1.2 specification
pub fn validate_manifest(manifest: &Manifest) -> Result<()> {
    // Validate schemaVersion
    const SUPPORTED_SCHEMA: [&str; 3] = ["1.0", "1.1", "1.2"];
    ensure!(
        SUPPORTED_SCHEMA.contains(&manifest.schema_version.as_str()),
        anyhow!(
            "unsupported manifest schemaVersion {}; expected one of {:?}",
            manifest.schema_version,
            SUPPORTED_SCHEMA
        )
    );

    validate_pack_spec(&manifest.pack)?;

    // Validate version.channel
    validate_channel(&manifest.version.channel)?;

    // Validate capabilities (? suffix check)
    for cap in &manifest.capabilities {
        validate_capability(cap)?;
    }

    // Validate egress_allow (URL format check) with egress_mode context
    for url in &manifest.network.egress_allow {
        validate_egress_url_with_mode(url, manifest.network.egress_mode.as_ref())?;
    }

    for (index, rule) in manifest.network.egress_id_allow.iter().enumerate() {
        let ctx = format!("network.egress_id_allow[{index}]");
        validate_egress_id_rule(rule, &ctx, manifest.network.egress_mode.as_ref())?;
    }

    for profile_name in &manifest.network.use_profiles {
        ensure!(
            manifest.network.profiles.contains_key(profile_name),
            anyhow!("network.use references unknown profile '{}'", profile_name)
        );
    }

    for (profile_name, profile) in &manifest.network.profiles {
        for url in &profile.egress_allow {
            validate_egress_url_with_mode(url, manifest.network.egress_mode.as_ref())?;
        }
        for (idx, rule) in profile.egress_id_allow.iter().enumerate() {
            let ctx = format!(
                "network.profiles['{}'].egress_id_allow[{}]",
                profile_name, idx
            );
            validate_egress_id_rule(rule, &ctx, manifest.network.egress_mode.as_ref())?;
        }
    }

    if let Some(x_cas) = &manifest.x_cas {
        validate_x_cas(x_cas)?;
    }

    for (index, capsule) in manifest.dep_capsules.iter().enumerate() {
        validate_dep_capsule(capsule, index)?;
    }

    if let Some(deps) = &manifest.deps {
        if let Some(python) = &deps.python {
            validate_python_deps(python)?;
        }
        if let Some(node) = &deps.node {
            validate_node_deps(node)?;
        }
    }

    for (index, file) in manifest.files.iter().enumerate() {
        validate_file_entry(file, index)?;
    }

    Ok(())
}

fn validate_pack_spec(pack: &PackSpec) -> Result<()> {
    if pack.is_empty() {
        return Ok(());
    }

    if let Some(profile) = pack.profile.as_deref() {
        const SUPPORTED_PROFILES: [&str; 3] = ["dist", "dist+cas", "frozen"];
        ensure!(
            SUPPORTED_PROFILES.contains(&profile),
            anyhow!(
                "pack.profile '{}' is unsupported; expected one of {:?}",
                profile,
                SUPPORTED_PROFILES
            )
        );
    }

    if let Some(pm) = pack.pm.as_ref() {
        ensure!(!pm.trim().is_empty(), "pack.pm cannot be an empty string");
    }

    for (engine, constraint) in &pack.engines {
        ensure!(
            !engine.trim().is_empty(),
            "pack.engines cannot contain empty engine keys"
        );
        ensure!(
            !constraint.trim().is_empty(),
            "pack.engines['{}'] must not be an empty string",
            engine
        );
    }

    Ok(())
}

/// Validate egress_allow URL with egress_mode context
fn validate_egress_url_with_mode(url: &str, egress_mode: Option<&EgressMode>) -> Result<()> {
    // 空文字列チェック
    if url.is_empty() {
        bail!("egress_allow URL cannot be empty");
    }

    // 外部URL（常に許可）
    if url.starts_with("https://") {
        return Ok(());
    }

    // WebSocket: Phase 2 で実装
    if url.starts_with("wss://") || url.starts_with("ws://") {
        bail!(
            "ADEP-WEBSOCKET-NOT-SUPPORTED: \n\
            WebSocket support is planned for Phase 2.\n\
            \n\
            Current URL: {}\n\
            \n\
            Phase 1A supports: https://, http://localhost: only",
            url
        );
    }

    // localhost URL（HTTP/HTTPS のみ）
    if url.starts_with("http://localhost:") || url.starts_with("http://127.0.0.1:") {
        if egress_mode != Some(&EgressMode::Dev) {
            bail!(
                "ADEP-LOCALHOST-REQUIRES-DEV-MODE: \n\
                localhost URLs require egress_mode: \"dev\" in manifest.json\n\
                \n\
                Current URL: {}\n\
                \n\
                Add to manifest.json:\n\
                \"network\": {{\n\
                  \"egress_mode\": \"dev\",\n\
                  ...\n\
                }}",
                url
            );
        }

        validate_localhost_url(url)?;
        return Ok(());
    }

    // 開発モードでは http:// も許可（LAN IP や Tailscale IP 用）
    if url.starts_with("http://") {
        if egress_mode != Some(&EgressMode::Dev) {
            bail!(
                "ADEP-HTTP-REQUIRES-DEV-MODE: \n\
                Non-HTTPS egress_allow entries must either use https:// or set egress_mode: \"dev\" in manifest.json\n\
                \n\
                Current URL: {}\n\
                \n\
                Add to manifest.json:\n\
                \"network\": {{\n\
                  \"egress_mode\": \"dev\",\n\
                  ...\n\
                }}",
                url
            );
        }

        // ワイルドカードチェック
        if url.contains('*') {
            // ワイルドカードは開発モードでのみ許可
            eprintln!("⚠️  WARNING: Using wildcard in egress_allow: {}", url);
            eprintln!("⚠️  This is allowed in dev mode but should be specific in production");
            eprintln!();
        }

        return Ok(());
    }

    bail!("egress_allow URL must start with https:// or http:// (dev mode only)");
}

fn validate_egress_id_rule(
    rule: &EgressIdRule,
    context: &str,
    egress_mode: Option<&EgressMode>,
) -> Result<()> {
    if rule.value.trim().is_empty() {
        bail!("{}: value cannot be empty", context);
    }

    match rule.rule_type {
        EgressIdType::AdepId | EgressIdType::AdepFamily => {
            Uuid::parse_str(&rule.value).with_context(|| {
                format!(
                    "{}: value must be a valid UUID for {:?}",
                    context, rule.rule_type
                )
            })?;
        }
        EgressIdType::Spiffe => {
            ensure!(
                rule.value.starts_with("spiffe://"),
                "{}: value must start with spiffe:// for spiffe rules",
                context
            );
        }
    }

    if rule.scheme.is_empty() {
        bail!("{}: scheme list must not be empty", context);
    }
    for scheme in &rule.scheme {
        match scheme.as_str() {
            "https" | "wss" => {}
            "http" if matches!(egress_mode, Some(EgressMode::Dev)) => {}
            other => {
                bail!(
                    "{}: unsupported scheme '{}'; allowed: https, wss (http only in dev mode)",
                    context,
                    other
                );
            }
        }
    }

    if rule.ports.is_empty() {
        bail!("{}: ports must contain at least one entry", context);
    }
    for port in &rule.ports {
        if *port == 0 {
            bail!("{}: port value 0 is invalid", context);
        }
    }

    for prefix in &rule.path_prefix {
        if !prefix.starts_with('/') {
            bail!("{}: path_prefix values must begin with '/'", context);
        }
    }

    if let Some(trust) = &rule.trust {
        validate_egress_trust(trust, context)?;
    } else {
        bail!(
            "{}: trust configuration is required (mode, credentials)",
            context
        );
    }

    Ok(())
}

fn validate_egress_trust(trust: &EgressTrust, context: &str) -> Result<()> {
    match trust.mode {
        TrustMode::Spiffe => {
            let id = trust.spiffe_id.as_deref().ok_or_else(|| {
                anyhow!("{}: trust.spiffe_id is required for mode=spiffe", context)
            })?;
            ensure!(
                id.starts_with("spiffe://"),
                "{}: trust.spiffe_id must start with spiffe://",
                context
            );
        }
        TrustMode::SpkiPin => {
            use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
            use base64::engine::Engine;

            let digest = trust.spki_sha256.as_deref().ok_or_else(|| {
                anyhow!(
                    "{}: trust.spki_sha256 is required for mode=spki-pin",
                    context
                )
            })?;
            let decoded = hex::decode(digest)
                .or_else(|_| STANDARD.decode(digest))
                .or_else(|_| URL_SAFE_NO_PAD.decode(digest))
                .with_context(|| {
                    format!(
                        "{}: trust.spki_sha256 must be hex or base64 encoded",
                        context
                    )
                })?;
            ensure!(
                decoded.len() == 32,
                "{}: trust.spki_sha256 must decode to 32 bytes (sha256 digest)",
                context
            );
        }
        TrustMode::CfAccess => {
            ensure!(
                trust.service_token_file.is_some(),
                "{}: trust.service_token_file is required for mode=cf-access",
                context
            );
        }
        TrustMode::Tailscale => {
            ensure!(
                trust.tailscale_tag.is_some()
                    || trust.extra.get("tag").and_then(Value::as_str).is_some(),
                "{}: trust.tailscale_tag (or tag) is required for mode=tailscale",
                context
            );
        }
    }
    Ok(())
}

fn validate_x_cas(x_cas: &CasConfig) -> Result<()> {
    if x_cas.index.trim().is_empty() {
        bail!("x-cas.index cannot be empty");
    }
    if x_cas.blobs.trim().is_empty() {
        bail!("x-cas.blobs cannot be empty");
    }
    if let Some(policy) = &x_cas.policy {
        if policy.trust_domain.trim().is_empty() {
            bail!("x-cas.policy.trustDomain cannot be empty");
        }
        for value in &policy.verify {
            ensure!(
                matches!(value.as_str(), "compressed" | "raw"),
                "x-cas.policy.verify contains unsupported value '{}'; allowed: compressed, raw",
                value
            );
        }
    }
    Ok(())
}

fn validate_dep_capsule(value: &str, index: usize) -> Result<()> {
    if value.trim().is_empty() {
        bail!("dep_capsules[{}] cannot be empty", index);
    }
    ensure!(
        value.starts_with("oci://") || value.starts_with("adep://"),
        "dep_capsules[{}] must start with oci:// or adep://",
        index
    );
    Ok(())
}

fn validate_python_deps(py: &PythonDependencies) -> Result<()> {
    if py.requirements.trim().is_empty() {
        bail!("deps.python.requirements cannot be empty");
    }
    if let Some(install) = &py.install {
        if install.mode.trim().is_empty() {
            bail!("deps.python.install.mode cannot be empty");
        }
        ensure!(
            install.mode == "offline",
            "deps.python.install.mode '{}' is unsupported (expected 'offline')",
            install.mode
        );
        if let Some(target) = &install.target {
            if target.trim().is_empty() {
                bail!("deps.python.install.target cannot be empty string");
            }
        }
        if let Some(no_deps) = install.no_deps {
            ensure!(
                no_deps,
                "deps.python.install.no_deps must be true when offline mode is used"
            );
        }
    }
    Ok(())
}

fn validate_node_deps(node: &NodeDependencies) -> Result<()> {
    if node.lockfile.trim().is_empty() {
        bail!("deps.node.lockfile cannot be empty");
    }
    if let Some(install) = &node.install {
        if install.mode.trim().is_empty() {
            bail!("deps.node.install.mode cannot be empty");
        }
        ensure!(
            install.mode == "offline",
            "deps.node.install.mode '{}' is unsupported (expected 'offline')",
            install.mode
        );
        if let Some(frozen) = install.frozen_lockfile {
            ensure!(
                frozen,
                "deps.node.install.frozen_lockfile must be true when offline mode is used"
            );
        }
    }
    Ok(())
}

fn validate_file_entry(file: &FileEntry, index: usize) -> Result<()> {
    if file.path.trim().is_empty() {
        bail!("files[{}].path cannot be empty", index);
    }
    ensure!(
        file.sha256.len() == 64 && file.sha256.chars().all(|c| c.is_ascii_hexdigit()),
        "files[{}].sha256 must be 64 hex characters",
        index
    );
    if let Some(compressed) = &file.compressed {
        ensure!(
            matches!(compressed.alg.as_str(), "zstd" | "brotli"),
            "files[{}].compressed.alg '{}' is unsupported (allowed: zstd, brotli)",
            index,
            compressed.alg
        );
        ensure!(
            compressed.size > 0,
            "files[{}].compressed.size must be greater than zero",
            index
        );
        ensure!(
            compressed.sha256.len() == 64
                && compressed.sha256.chars().all(|c| c.is_ascii_hexdigit()),
            "files[{}].compressed.sha256 must be 64 hex characters",
            index
        );
    }
    Ok(())
}

/// Validate that channel is one of: stable, beta, canary
fn validate_channel(channel: &str) -> Result<()> {
    match channel {
        "stable" | "beta" | "canary" => Ok(()),
        _ => bail!(
            "invalid channel `{}`; must be one of: stable, beta, canary",
            channel
        ),
    }
}

/// Validate capability syntax (must end with ? if it's a permission)
fn validate_capability(cap: &str) -> Result<()> {
    // 空文字列チェック
    if cap.is_empty() {
        bail!("capability cannot be empty");
    }

    // ADEP仕様: パーミッション系capabilityは ? で終わるべき
    // 許可されたパターン:
    // - "storage?" (パーミッション要求)
    // - "widget" (機能フラグ、? なし)

    // 基本的な文字チェック（英数字、ハイフン、アンダースコア、ドット、?のみ）
    // ドット(.)は名前空間区切りとして許可（例: fs.cache, network.fetch）
    for c in cap.chars() {
        if !c.is_alphanumeric() && c != '-' && c != '_' && c != '.' && c != '?' {
            bail!("capability `{}` contains invalid character `{}`", cap, c);
        }
    }

    // ? が複数含まれている場合はエラー
    if cap.matches('?').count() > 1 {
        bail!("capability `{}` contains multiple `?` characters", cap);
    }

    // ? が含まれる場合は末尾でなければエラー
    if cap.contains('?') && !cap.ends_with('?') {
        bail!("capability `{}` has `?` but not at the end", cap);
    }

    Ok(())
}

// 真に危険なポートのみブロック（範囲制限は削除）
const BLOCKED_PORTS: &[u16] = &[
    22,  // SSH - リモート管理の乗っ取りリスク
    25,  // SMTP - スパムメール送信リスク
    80,  // HTTP - システムサービスと衝突
    443, // HTTPS - 同上
];

/// Validate localhost URL (HTTP/HTTPS only, with port requirement)
fn validate_localhost_url(url: &str) -> Result<()> {
    use url::Url;

    let parsed = Url::parse(url).context("Failed to parse URL")?;

    // 1. ポート番号必須
    let port = parsed
        .port()
        .ok_or_else(|| anyhow!("ADEP-LOCALHOST-NO-PORT: localhost URLs must specify a port"))?;

    // 2. ブロックリストチェック（厳格）
    if BLOCKED_PORTS.contains(&port) {
        bail!(
            "ADEP-LOCALHOST-BLOCKED-PORT: Port {} is blocked for security reasons.\n\
            Blocked ports: {:?}\n\
            \n\
            These ports are reserved for system services.",
            port,
            BLOCKED_PORTS
        );
    }

    // 3. 警告のみ（特権ポート）
    if port < 1024 {
        eprintln!("⚠️  WARNING: Using privileged port {}", port);
        eprintln!("⚠️  May require root/admin permissions");
        eprintln!();
    }

    // 4. ワイルドカード禁止
    if url.contains('*') {
        bail!("ADEP-LOCALHOST-WILDCARD: Wildcard patterns are not allowed");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_developer_key_success() {
        let key = "ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        assert!(parse_developer_key(key).is_ok());
    }

    #[test]
    fn parse_developer_key_requires_prefix() {
        assert!(parse_developer_key("invalid").is_err());
    }

    #[test]
    fn test_validate_localhost_url_success() {
        assert!(validate_localhost_url("http://localhost:8000").is_ok());
        assert!(validate_localhost_url("http://localhost:3000").is_ok());
        assert!(validate_localhost_url("http://127.0.0.1:8080").is_ok());
    }

    #[test]
    fn test_blocked_system_ports() {
        assert!(validate_localhost_url("http://localhost:22").is_err());
        assert!(validate_localhost_url("http://localhost:25").is_err());
        assert!(validate_localhost_url("http://localhost:80").is_err());
        assert!(validate_localhost_url("http://localhost:443").is_err());
    }

    #[test]
    fn test_warned_ports_allowed() {
        // DBポートは警告のみ（ブロックはしない）
        assert!(validate_localhost_url("http://localhost:3306").is_ok());
        assert!(validate_localhost_url("http://localhost:5432").is_ok());
    }

    #[test]
    fn test_port_no_range_restriction() {
        // 範囲制限は削除、ブロックリスト以外は許可
        assert!(validate_localhost_url("http://localhost:3000").is_ok());
        assert!(validate_localhost_url("http://localhost:9999").is_ok());
        assert!(validate_localhost_url("http://localhost:2999").is_ok()); // OK
        assert!(validate_localhost_url("http://localhost:10000").is_ok()); // OK
        assert!(validate_localhost_url("http://localhost:3306").is_ok()); // MySQL OK
    }

    #[test]
    fn test_localhost_url_requires_port() {
        // ポート番号なしは拒否
        assert!(validate_localhost_url("http://localhost").is_err());
        assert!(validate_localhost_url("http://localhost/").is_err());
    }

    #[test]
    fn test_validate_egress_url_with_mode_https() {
        // https:// は常に許可
        assert!(validate_egress_url_with_mode("https://api.example.com", None).is_ok());
        assert!(validate_egress_url_with_mode("https://api.example.com/path", None).is_ok());
    }

    #[test]
    fn validate_manifest_fixtures_across_versions() {
        use std::fs;
        use std::path::Path;

        let fixtures = [
            ("v1_0", "manifest.min.json"),
            ("v1_1", "manifest.min.json"),
            ("v1_2", "manifest.min.json"),
            ("v1_2", "manifest.full.json"),
        ];

        for (version, file_name) in fixtures {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join("manifests")
                .join(version)
                .join(file_name);
            let manifest_json = fs::read_to_string(&path).unwrap_or_else(|err| {
                panic!(
                    "failed to load manifest fixture for {}: {} ({})",
                    version,
                    path.display(),
                    err
                )
            });
            let manifest: Manifest = serde_json::from_str(&manifest_json).unwrap_or_else(|err| {
                panic!(
                    "failed to parse manifest fixture for {}: {} ({})",
                    version,
                    path.display(),
                    err
                )
            });

            validate_manifest(&manifest)
                .unwrap_or_else(|err| panic!("validation failed for {}: {}", version, err));
        }

        let invalid_fixtures = [
            ("invalid", "manifest_localhost_without_dev.json"),
            ("invalid", "manifest_missing_trust.json"),
            ("invalid", "manifest_invalid_dep_capsule.json"),
            ("invalid", "manifest_invalid_python_no_deps.json"),
            ("invalid", "manifest_invalid_compressed.json"),
        ];

        for (dir, file_name) in invalid_fixtures {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join("manifests")
                .join(dir)
                .join(file_name);
            let manifest_json = fs::read_to_string(&path).unwrap_or_else(|err| {
                panic!(
                    "failed to load invalid manifest fixture {}: {} ({})",
                    file_name,
                    path.display(),
                    err
                )
            });
            let manifest: Manifest = serde_json::from_str(&manifest_json).unwrap_or_else(|err| {
                panic!(
                    "failed to parse invalid manifest fixture {}: {} ({})",
                    file_name,
                    path.display(),
                    err
                )
            });

            assert!(
                validate_manifest(&manifest).is_err(),
                "expected validation failure for {}",
                file_name
            );
        }
    }

    #[test]
    fn test_validate_egress_url_with_mode_websocket_blocked() {
        // WebSocket は Phase 2 で実装
        assert!(
            validate_egress_url_with_mode("ws://localhost:8080", Some(&EgressMode::Dev)).is_err()
        );
        assert!(validate_egress_url_with_mode("wss://example.com", None).is_err());
    }

    #[test]
    fn test_validate_egress_url_with_mode_localhost_requires_dev() {
        // localhost は dev mode 必須
        assert!(validate_egress_url_with_mode("http://localhost:8000", None).is_err());
        assert!(
            validate_egress_url_with_mode("http://localhost:8000", Some(&EgressMode::Dev)).is_ok()
        );
    }

    #[test]
    fn test_egress_mode_serialization() {
        // enum のシリアライズ/デシリアライズ
        let mode = EgressMode::Dev;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"dev\"");

        let deserialized: EgressMode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, EgressMode::Dev);
    }

    #[test]
    fn test_dependencies_serialization() {
        // Dependencies のシリアライズ/デシリアライズ
        let deps = Dependencies {
            adep: vec![AdepDependency {
                name: "api".to_string(),
                family_id: "uuid-123".to_string(),
                port: Some(8000),
            }],
        };

        let json = serde_json::to_string(&deps).unwrap();
        let deserialized: Dependencies = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.adep.len(), 1);
        assert_eq!(deserialized.adep[0].name, "api");
        assert_eq!(deserialized.adep[0].port, Some(8000));
    }
}
