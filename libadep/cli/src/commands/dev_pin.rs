use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use base64::{decode, decode_config, encode_config, URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use clap::{Args, Subcommand};
use dirs::home_dir;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;

use crate::error;

const PIN_VERSION: &str = "1.1";
const MAC_ALG: &str = "HMAC-SHA-256";
const KEY_BYTES: usize = 32;
const DEFAULT_TTL_MINUTES: i64 = 20;

type HmacSha256 = Hmac<Sha256>;

#[derive(Args, Debug, Clone)]
pub struct DevPinArgs {
    #[command(subcommand)]
    pub command: DevPinCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum DevPinCommand {
    /// Show registered Dev/CI pin records
    Status,
    /// Create or refresh a pin record for a specific target
    Repin(RepinArgs),
    /// Remove pin records (expired or all)
    Purge(PurgeArgs),
}

#[derive(Args, Debug, Clone)]
pub struct RepinArgs {
    /// Target identifier (usually hostname or logical service name)
    pub target: String,
    /// Primary SAN / SPIFFE ID (comma separated for multiple)
    #[arg(long)]
    pub san: Option<String>,
    /// SPKI SHA-256 fingerprint (base64 or hex)
    #[arg(long)]
    pub spki: Option<String>,
    /// TTL in minutes (default 20, max 30)
    #[arg(long, default_value_t = DEFAULT_TTL_MINUTES)]
    pub ttl: i64,
    /// Resolver scope (user | project | ci-job)
    #[arg(long, default_value = "user")]
    pub scope: String,
    /// Pin source (tofu | admin | ci)
    #[arg(long, default_value = "tofu")]
    pub source: String,
    /// Resolver mode (defaults to adhoc-pinned)
    #[arg(long, default_value = "adhoc-pinned")]
    pub resolver_mode: String,
    /// Optional trust domain hint
    #[arg(long)]
    pub trust_domain: Option<String>,
    /// Optional scheme hint (default https)
    #[arg(long)]
    pub scheme: Option<String>,
    /// Optional port hint
    #[arg(long)]
    pub port: Option<u16>,
    /// Optional associated egress_id
    #[arg(long)]
    pub egress_id: Option<String>,
    /// Disable mTLS requirement (not recommended)
    #[arg(long)]
    pub no_mtls: bool,
}

#[derive(Args, Debug, Clone)]
pub struct PurgeArgs {
    /// Remove all pin records
    #[arg(long)]
    pub all: bool,
    /// Remove expired pin records (default behaviour)
    #[arg(long)]
    pub expired: bool,
}

pub fn run(args: DevPinArgs) -> Result<()> {
    match args.command {
        DevPinCommand::Status => run_status(),
        DevPinCommand::Repin(cmd) => run_repin(cmd),
        DevPinCommand::Purge(cmd) => run_purge(cmd),
    }
}

fn run_status() -> Result<()> {
    let store = PinStore::open()?;
    let state = store.load_state()?;

    if state.entries.is_empty() {
        println!("No Dev/CI pin records found.");
        return Ok(());
    }

    let now = Utc::now();
    let mut entries: Vec<&PinEntry> = state.entries.iter().collect();
    entries.sort_by_key(|entry| entry.data.expires_at);

    println!(
        "{:<24} {:<22} {:<12} {:<10} {:<32}",
        "Target", "Expires (UTC)", "Remaining", "Scope", "Identity"
    );
    println!("{}", "-".repeat(108));

    for entry in entries {
        let expires = entry.data.expires_at.format("%Y-%m-%d %H:%M:%S");
        let remaining = format_remaining(entry.data.expires_at, now);
        let scope = &entry.data.scope;
        let identity = render_identity_hint(&entry.data.identity_hint);
        println!(
            "{:<24} {:<22} {:<12} {:<10} {:<32}",
            entry.data.target, expires, remaining, scope, identity
        );
    }

    Ok(())
}

fn run_repin(args: RepinArgs) -> Result<()> {
    if args.ttl <= 0 {
        bail!("TTL must be positive minutes");
    }
    if args.ttl > 30 {
        bail!("TTL cannot exceed 30 minutes for adhoc-pinned mode");
    }

    let store = PinStore::open()?;
    let mut state = store.load_state()?;

    let san_input = match args.san {
        Some(value) if !value.trim().is_empty() => value,
        _ => prompt("SAN / SPIFFE URI (comma separated, leave blank to skip): ")?,
    };
    let san_uris = parse_san_list(&san_input);

    let spki_input = match args.spki {
        Some(value) if !value.trim().is_empty() => value,
        _ => prompt("SPKI SHA-256 (base64 or hex, required): ")?,
    };
    let spki_sha256 = normalize_spki(&spki_input)?;

    let resolver_mode = args.resolver_mode.trim().to_string();
    if resolver_mode.is_empty() {
        bail!("resolver_mode cannot be empty");
    }

    let ttl = Duration::minutes(args.ttl);
    let now = Utc::now();
    let expires_at = now + ttl;

    let hint = PinIdentityHint {
        san_uris,
        trust_domain: args.trust_domain.clone(),
        sni: Some(args.target.clone()),
        scheme: Some(args.scheme.unwrap_or_else(|| "https".into())),
        port: args.port,
    };

    let policy = PinPolicy {
        mtls_required: !args.no_mtls,
        egress_id: args.egress_id.clone(),
    };

    let data = PinRecordData {
        version: PIN_VERSION.to_string(),
        created_at: now,
        expires_at,
        scope: args.scope.clone(),
        source: args.source.clone(),
        target: args.target.clone(),
        resolver_mode: resolver_mode.clone(),
        meta: None,
        peer: PinPeer { spki_sha256 },
        identity_hint: hint,
        policy,
        mac_alg: MAC_ALG.to_string(),
    };

    let entry = PinEntry::new(data, state.key())?;
    state.upsert(entry);
    state.save(&store)?;

    println!(
        "Pinned '{}' for resolver '{}' (expires in {}).",
        args.target,
        resolver_mode,
        format_remaining(expires_at, now)
    );
    Ok(())
}

fn run_purge(args: PurgeArgs) -> Result<()> {
    let store = PinStore::open()?;
    let mut state = store.load_state()?;

    let purge_all = args.all;
    let purge_expired = args.expired || !purge_all;

    let removed = if purge_all {
        let count = state.entries.len();
        state.entries.clear();
        count
    } else if purge_expired {
        state.purge_expired()
    } else {
        0
    };

    if removed == 0 {
        println!("No pin records removed.");
    } else {
        state.save(&store)?;
        println!("Removed {} pin record(s).", removed);
    }

    Ok(())
}

fn prompt(message: &str) -> Result<String> {
    print!("{}", message);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn parse_san_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn normalize_spki(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("SPKI fingerprint is required");
    }

    // Accept colon-separated or plain hex (64 chars)
    let mut hex_candidate = trimmed.replace(':', "");
    hex_candidate.retain(|c| !c.is_whitespace());
    if hex_candidate.len() == 64 && hex_candidate.chars().all(|c| c.is_ascii_hexdigit()) {
        let bytes =
            hex::decode(&hex_candidate).with_context(|| "failed to decode hex SPKI fingerprint")?;
        return Ok(encode_config(bytes, URL_SAFE_NO_PAD));
    }

    let decoded = decode_config(trimmed, URL_SAFE_NO_PAD)
        .or_else(|_| decode(trimmed))
        .with_context(|| "failed to decode SPKI fingerprint (expected base64 or hex)")?;

    if decoded.len() != 32 {
        bail!(
            "SPKI fingerprint must decode to 32 bytes (sha256 digest), got {} bytes",
            decoded.len()
        );
    }

    Ok(encode_config(decoded, URL_SAFE_NO_PAD))
}

fn format_remaining(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = expires_at - now;
    if delta.num_seconds() <= 0 {
        "expired".into()
    } else if delta.num_minutes() < 1 {
        format!("{}s", delta.num_seconds())
    } else if delta.num_minutes() < 120 {
        format!("{}m", delta.num_minutes())
    } else {
        let hours = delta.num_hours();
        format!("{}h", hours)
    }
}

fn render_identity_hint(hint: &PinIdentityHint) -> String {
    if let Some(domain) = &hint.trust_domain {
        domain.clone()
    } else if let Some(sni) = &hint.sni {
        sni.clone()
    } else if !hint.san_uris.is_empty() {
        let first = hint.san_uris[0].clone();
        if hint.san_uris.len() > 1 {
            format!("{} (+{})", first, hint.san_uris.len() - 1)
        } else {
            first
        }
    } else {
        "-".into()
    }
}

struct PinStore {
    pins_path: PathBuf,
    key_path: PathBuf,
}

impl PinStore {
    fn open() -> Result<Self> {
        let home = home_dir().ok_or_else(|| error::home_dir_unavailable())?;
        let dir = home.join(".adep").join("dev-pins");
        ensure_dir(&dir)?;

        Ok(Self {
            pins_path: dir.join("pins.json"),
            key_path: dir.join("key.bin"),
        })
    }

    fn load_state(&self) -> Result<PinState> {
        let key = self.load_or_create_key()?;
        let entries = self.load_records(&key)?;

        let mut state = PinState { key, entries };
        let removed = state.cleanup_expired();
        if removed > 0 {
            state.save(self)?;
            eprintln!("⚠️  Removed {} expired pin record(s).", removed);
        }

        Ok(state)
    }

    fn load_or_create_key(&self) -> Result<Vec<u8>> {
        if let Ok(bytes) = fs::read(&self.key_path) {
            if bytes.len() == KEY_BYTES {
                return Ok(bytes);
            }
        }

        let mut key = vec![0u8; KEY_BYTES];
        OsRng.fill_bytes(&mut key);

        ensure_parent(&self.key_path)?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.key_path)
            .with_context(|| format!("failed to create {}", self.key_path.display()))?;
        file.write_all(&key)?;
        file.flush()?;
        set_permissions_file(&self.key_path)?;

        Ok(key)
    }

    fn load_records(&self, key: &[u8]) -> Result<Vec<PinEntry>> {
        if !self.pins_path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.pins_path)
            .with_context(|| format!("failed to open {}", self.pins_path.display()))?;
        let reader = BufReader::new(file);
        let records: Vec<PinRecordSigned> = serde_json::from_reader(reader)
            .with_context(|| format!("failed to parse {}", self.pins_path.display()))?;

        let mut entries = Vec::new();
        for record in records {
            if record.data.mac_alg.to_uppercase() != MAC_ALG {
                eprintln!(
                    "⚠️  Skipping pin for {} due to unsupported mac_alg {}",
                    record.data.target, record.data.mac_alg
                );
                continue;
            }

            let expected = compute_mac(key, &record.data)?;
            if expected != record.mac {
                eprintln!(
                    "⚠️  Skipping pin for {} due to MAC mismatch",
                    record.data.target
                );
                continue;
            }

            entries.push(PinEntry {
                data: record.data,
                mac: record.mac,
            });
        }

        Ok(entries)
    }

    fn write_records(&self, entries: &[PinEntry]) -> Result<()> {
        ensure_parent(&self.pins_path)?;
        let tmp = self.pins_path.with_extension("tmp");
        {
            let file =
                File::create(&tmp).with_context(|| format!("failed to write {}", tmp.display()))?;
            set_permissions_file(&tmp)?;
            let writer = BufWriter::new(file);
            let serialized: Vec<PinRecordSigned> = entries
                .iter()
                .map(|entry| PinRecordSigned {
                    data: entry.data.clone(),
                    mac: entry.mac.clone(),
                })
                .collect();
            serde_json::to_writer_pretty(writer, &serialized)
                .with_context(|| format!("failed to serialize {}", tmp.display()))?;
        }

        fs::rename(&tmp, &self.pins_path)
            .with_context(|| format!("failed to replace {}", self.pins_path.display()))?;
        set_permissions_file(&self.pins_path)?;
        Ok(())
    }
}

struct PinState {
    key: Vec<u8>,
    entries: Vec<PinEntry>,
}

impl PinState {
    fn key(&self) -> &[u8] {
        &self.key
    }

    fn cleanup_expired(&mut self) -> usize {
        let now = Utc::now();
        let before = self.entries.len();
        self.entries.retain(|entry| entry.data.expires_at > now);
        before - self.entries.len()
    }

    fn upsert(&mut self, entry: PinEntry) {
        self.entries.retain(|existing| {
            !(existing.data.target == entry.data.target
                && existing.data.resolver_mode == entry.data.resolver_mode)
        });
        self.entries.push(entry);
    }

    fn purge_expired(&mut self) -> usize {
        let now = Utc::now();
        let before = self.entries.len();
        self.entries.retain(|entry| entry.data.expires_at > now);
        before - self.entries.len()
    }

    fn save(&mut self, store: &PinStore) -> Result<()> {
        for entry in &mut self.entries {
            entry.mac = compute_mac(&self.key, &entry.data)?;
        }
        store.write_records(&self.entries)
    }
}

#[derive(Debug, Clone)]
struct PinEntry {
    data: PinRecordData,
    mac: String,
}

impl PinEntry {
    fn new(data: PinRecordData, key: &[u8]) -> Result<Self> {
        let mac = compute_mac(key, &data)?;
        Ok(Self { data, mac })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PinRecordSigned {
    #[serde(flatten)]
    data: PinRecordData,
    mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PinRecordData {
    version: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    scope: String,
    source: String,
    target: String,
    resolver_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    meta: Option<Value>,
    peer: PinPeer,
    #[serde(default)]
    identity_hint: PinIdentityHint,
    #[serde(default)]
    policy: PinPolicy,
    #[serde(default = "default_mac_alg")]
    mac_alg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PinPeer {
    spki_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PinIdentityHint {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    san_uris: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trust_domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sni: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PinPolicy {
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    mtls_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    egress_id: Option<String>,
}

fn compute_mac(key: &[u8], data: &PinRecordData) -> Result<String> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|_| anyhow!("invalid HMAC key length"))?;
    let payload = serde_json::to_vec(data)?;
    mac.update(&payload);
    let result = mac.finalize().into_bytes();
    Ok(encode_config(result, URL_SAFE_NO_PAD))
}

fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))?;
    }
    set_permissions_dir(path)?;
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_permissions_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o700);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_permissions_dir(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_permissions_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn set_permissions_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn default_mac_alg() -> String {
    MAC_ALG.to_string()
}
