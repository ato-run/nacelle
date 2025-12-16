use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const AUDIT_LOG_ENV: &str = "ADEP_AUDIT_LOG";
const METRICS_LOG_ENV: &str = "ADEP_METRICS_LOG";

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AuditEvent<'a> {
    pub ts: String,
    pub component: &'a str,
    pub event: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coords: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<&'a str>,
}

#[derive(Clone)]
pub struct AuditWriter {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl AuditWriter {
    pub fn new(path: Option<PathBuf>) -> Result<Self> {
        let path = match path.or_else(read_audit_env) {
            Some(p) => p,
            None => default_audit_path()?,
        };
        if let Some(parent) = path.parent() {
            create_dir_all(parent).with_context(|| {
                format!("failed to create audit log directory {}", parent.display())
            })?;
        }
        Ok(Self {
            path,
            lock: Arc::new(Mutex::new(())),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write_event(&self, event: &AuditEvent<'_>) -> Result<()> {
        let _guard = self.lock.lock().expect("audit mutex poisoned");
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)
            .with_context(|| format!("unable to open audit log {}", self.path.display()))?;
        let mut value = serde_json::to_value(event)?;
        if !value
            .get("ts")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false)
        {
            value["ts"] =
                Value::String(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
        }
        let line = serde_json::to_string(&value)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        Ok(())
    }
}

fn read_audit_env() -> Option<PathBuf> {
    std::env::var_os(AUDIT_LOG_ENV).map(PathBuf::from)
}

fn default_audit_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("HOME directory not found"))?;
    Ok(home.join(".adep/logs/deps.audit.jsonl"))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct MetricKey {
    name: String,
    labels: Vec<(String, String)>,
}

impl MetricKey {
    fn new(name: &str, labels: &[(&str, &str)]) -> Self {
        let mut labels_vec: Vec<(String, String)> = labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        labels_vec.sort();
        Self {
            name: name.to_string(),
            labels: labels_vec,
        }
    }
}

#[derive(Default)]
struct MetricsState {
    counters: BTreeMap<MetricKey, f64>,
    gauges: BTreeMap<MetricKey, f64>,
}

impl MetricsState {
    fn encode(&self) -> String {
        let mut lines = Vec::new();
        for (key, value) in &self.counters {
            lines.push(format!(
                "{}{} {}",
                key.name,
                format_labels(&key.labels),
                *value
            ));
        }
        for (key, value) in &self.gauges {
            lines.push(format!(
                "{}{} {}",
                key.name,
                format_labels(&key.labels),
                *value
            ));
        }
        lines.sort();
        lines.join("\n")
    }
}

fn format_labels(labels: &[(String, String)]) -> String {
    if labels.is_empty() {
        String::new()
    } else {
        let rendered: Vec<String> = labels
            .iter()
            .map(|(k, v)| format!(r#"{k}="{v}""#))
            .collect();
        format!("{{{}}}", rendered.join(","))
    }
}

#[derive(Clone)]
pub struct MetricsRegistry {
    path: Option<PathBuf>,
    state: Arc<Mutex<MetricsState>>,
}

impl MetricsRegistry {
    pub fn new(path: Option<PathBuf>) -> Result<Self> {
        let path = match path.or_else(read_metrics_env) {
            Some(p) => {
                if let Some(parent) = p.parent() {
                    create_dir_all(parent).with_context(|| {
                        format!("failed to create metrics directory {}", parent.display())
                    })?;
                }
                Some(p)
            }
            None => None,
        };
        Ok(Self {
            path,
            state: Arc::new(Mutex::new(MetricsState::default())),
        })
    }

    pub fn inc_counter(&self, name: &str, labels: &[(&str, &str)], delta: f64) -> Result<()> {
        let mut state = self.state.lock().expect("metrics mutex poisoned");
        let key = MetricKey::new(name, labels);
        *state.counters.entry(key).or_insert(0.0) += delta;
        drop(state);
        self.flush()
    }

    pub fn set_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64) -> Result<()> {
        let mut state = self.state.lock().expect("metrics mutex poisoned");
        let key = MetricKey::new(name, labels);
        state.gauges.insert(key, value);
        drop(state);
        self.flush()
    }

    pub fn encode(&self) -> String {
        let state = self.state.lock().expect("metrics mutex poisoned");
        state.encode()
    }

    pub fn flush(&self) -> Result<()> {
        let path = match &self.path {
            Some(path) => path,
            None => return Ok(()),
        };
        let encoded = {
            let state = self.state.lock().expect("metrics mutex poisoned");
            state.encode()
        };
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .with_context(|| format!("unable to open metrics file {}", path.display()))?;
        if !encoded.is_empty() {
            file.write_all(encoded.as_bytes())?;
            file.write_all(b"\n")?;
        }
        Ok(())
    }
}

fn read_metrics_env() -> Option<PathBuf> {
    std::env::var_os(METRICS_LOG_ENV).map(PathBuf::from)
}
