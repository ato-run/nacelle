#![allow(dead_code)]

use crate::{safety, CasError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CompressedEntry {
    pub alg: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(alias = "sha256", default)]
    pub digest: Option<String>,
}

impl CompressedEntry {
    pub fn digest(&self) -> Option<&str> {
        self.digest.as_deref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct IndexEntry {
    pub path: String,
    #[serde(default)]
    pub coords: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<IndexMetadata>,
    #[serde(alias = "sha256")]
    pub raw_sha256: String,
    #[serde(default)]
    pub compressed_sha256: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub platform: Vec<String>,
    #[serde(default)]
    pub compressed: Option<CompressedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct IndexMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl IndexEntry {
    pub fn normalize(mut self) -> Result<Self, CasError> {
        if self.raw_sha256.is_empty() {
            return Err(CasError::InvalidIndex(format!(
                "missing raw_sha256 for path '{}'",
                self.path
            )));
        }
        if let Some(meta) = self.metadata.as_mut() {
            if let Some(filename) = meta.filename.as_mut() {
                let trimmed = filename.trim();
                if trimmed.is_empty() {
                    return Err(CasError::InvalidIndex(format!(
                        "metadata.filename must not be empty for path '{}'",
                        self.path
                    )));
                }
                *filename = trimmed.to_string();
            }
            if let Some(kind) = meta.kind.as_mut() {
                let trimmed = kind.trim();
                if trimmed.is_empty() {
                    return Err(CasError::InvalidIndex(format!(
                        "metadata.kind must not be empty for path '{}'",
                        self.path
                    )));
                }
                *kind = trimmed.to_string();
            }
        }
        if self.compressed_sha256.is_some() && self.compressed.is_none() {
            return Err(CasError::InvalidIndex(format!(
                "compressed_sha256 provided without compression metadata for path '{}'",
                self.path
            )));
        }
        if let Some(comp) = self.compressed.as_mut() {
            if comp.alg.trim().is_empty() {
                return Err(CasError::InvalidIndex(format!(
                    "missing compression algorithm for path '{}'",
                    self.path
                )));
            }
            if self.compressed_sha256.is_none() {
                if let Some(digest) = comp.digest() {
                    self.compressed_sha256 = Some(digest.to_string());
                }
            } else if let Some(digest) = comp.digest() {
                if Some(digest) != self.compressed_sha256.as_deref() {
                    return Err(CasError::InvalidIndex(format!(
                        "compressed_sha256 mismatch for path '{}'",
                        self.path
                    )));
                }
            }
            if comp.digest.is_none() {
                if let Some(digest) = &self.compressed_sha256 {
                    comp.digest = Some(digest.clone());
                }
            }
            if let (Some(raw_size), Some(compressed_size)) = (self.size, comp.size) {
                safety::enforce_compression_ratio(raw_size, compressed_size)?;
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalIndex {
    entries: Vec<IndexEntry>,
}

impl CanonicalIndex {
    pub fn new(entries: Vec<IndexEntry>) -> Self {
        let mut sorted = entries;
        sorted.sort_by(|a, b| a.path.cmp(&b.path));
        Self { entries: sorted }
    }

    pub fn from_entries(entries: Vec<IndexEntry>) -> Result<Self, CasError> {
        let mut normalized = Vec::with_capacity(entries.len());
        for entry in entries {
            normalized.push(entry.normalize()?);
        }
        Ok(Self::new(normalized))
    }

    pub fn entries(&self) -> &[IndexEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn push(&mut self, entry: IndexEntry) -> Result<(), CasError> {
        self.entries.push(entry.normalize()?);
        self.entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(())
    }

    pub fn from_reader(mut reader: impl Read) -> Result<Self, CasError> {
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        Self::from_json_str(&buf)
    }

    pub fn from_json_str(value: &str) -> Result<Self, CasError> {
        let raw_entries: Vec<IndexEntry> = serde_json::from_str(value)
            .map_err(|err| CasError::InvalidIndex(format!("invalid index json: {err}")))?;
        Self::from_entries(raw_entries)
    }

    pub fn duplicates(&self) -> Vec<Duplicate<'_>> {
        let mut duplicates = Vec::new();
        let mut by_path: BTreeMap<&str, Vec<&IndexEntry>> = BTreeMap::new();
        let mut by_compressed: BTreeMap<&str, Vec<&IndexEntry>> = BTreeMap::new();
        let mut by_raw: BTreeMap<&str, Vec<&IndexEntry>> = BTreeMap::new();
        let mut by_coord: BTreeMap<&str, Vec<&IndexEntry>> = BTreeMap::new();

        for entry in &self.entries {
            by_path.entry(&entry.path).or_default().push(entry);
            if let Some(digest) = entry.compressed_sha256.as_deref() {
                by_compressed.entry(digest).or_default().push(entry);
            }
            by_raw.entry(&entry.raw_sha256).or_default().push(entry);
            for coord in &entry.coords {
                by_coord.entry(coord).or_default().push(entry);
            }
        }

        for (path, group) in by_path {
            if group.len() > 1 {
                duplicates.push(Duplicate {
                    kind: DuplicateKind::Path(path),
                    entries: group,
                });
            }
        }

        for (digest, group) in by_compressed {
            if has_inconsistent_raw_or_size(&group) {
                duplicates.push(Duplicate {
                    kind: DuplicateKind::CompressedSha256(digest),
                    entries: group,
                });
            }
        }

        for (raw, group) in by_raw {
            if has_inconsistent_compressed(&group) {
                duplicates.push(Duplicate {
                    kind: DuplicateKind::RawSha256(raw),
                    entries: group,
                });
            }
        }

        for (coord, group) in by_coord {
            if has_inconsistent_compressed(&group) {
                duplicates.push(Duplicate {
                    kind: DuplicateKind::Coord(coord),
                    entries: group,
                });
            }
        }

        duplicates
    }

    pub fn diff<'a>(&'a self, other: &'a CanonicalIndex) -> IndexDiff<'a> {
        let mut self_map: BTreeMap<&str, &IndexEntry> = BTreeMap::new();
        let mut other_map: BTreeMap<&str, &IndexEntry> = BTreeMap::new();

        for entry in &self.entries {
            self_map.insert(&entry.path, entry);
        }
        for entry in &other.entries {
            other_map.insert(&entry.path, entry);
        }

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut changed = Vec::new();

        for (path, next) in &other_map {
            match self_map.get(path) {
                None => added.push(*next),
                Some(prev) if *prev != *next => changed.push(IndexChange {
                    path,
                    previous: *prev,
                    next,
                }),
                _ => {}
            }
        }

        for (path, prev) in &self_map {
            if !other_map.contains_key(path) {
                removed.push(*prev);
            }
        }

        IndexDiff {
            added,
            removed,
            changed,
        }
    }

    pub fn merge<I>(&self, incoming: I) -> Result<MergeReport, CasError>
    where
        I: IntoIterator<Item = IndexEntry>,
    {
        let mut merged_map: BTreeMap<String, IndexEntry> = self
            .entries
            .iter()
            .cloned()
            .map(|entry| (entry.path.clone(), entry))
            .collect();

        let mut coord_index: HashMap<String, (Option<String>, String)> = HashMap::new();
        let mut compressed_index: HashMap<String, (String, Option<u64>, String)> = HashMap::new();
        let mut raw_index: HashMap<String, (Option<String>, Option<u64>, String)> = HashMap::new();

        for entry in merged_map.values() {
            register_indexes(
                entry,
                &mut coord_index,
                &mut compressed_index,
                &mut raw_index,
            );
        }

        let mut added = Vec::new();
        let mut updated = Vec::new();
        let mut unchanged = Vec::new();
        let mut conflicts = Vec::new();
        let mut seen_paths: HashSet<String> = HashSet::new();

        for entry in incoming {
            let normalized = entry.normalize()?;
            if !seen_paths.insert(normalized.path.clone()) {
                let existing = merged_map
                    .get(&normalized.path)
                    .cloned()
                    .unwrap_or_else(|| normalized.clone());
                conflicts.push(MergeConflict {
                    kind: MergeConflictKind::Path,
                    key: normalized.path.clone(),
                    existing,
                    incoming: normalized.clone(),
                    reason: "duplicate path in incoming entries".into(),
                });
                continue;
            }

            if let Some(conflict) = find_coord_conflict(&normalized, &coord_index, &merged_map) {
                conflicts.push(conflict.with_incoming(normalized.clone()));
                continue;
            }

            if let Some(conflict) =
                find_compressed_conflict(&normalized, &compressed_index, &merged_map)
            {
                conflicts.push(conflict.with_incoming(normalized.clone()));
                continue;
            }

            if let Some(conflict) = find_raw_conflict(&normalized, &raw_index, &merged_map) {
                conflicts.push(conflict.with_incoming(normalized.clone()));
                continue;
            }

            match merged_map.get_mut(&normalized.path) {
                Some(existing) if *existing == normalized => {
                    unchanged.push(normalized.clone());
                }
                Some(existing) => {
                    unregister_indexes(
                        existing,
                        &mut coord_index,
                        &mut compressed_index,
                        &mut raw_index,
                    );
                    let previous = existing.clone();
                    *existing = normalized.clone();
                    register_indexes(
                        existing,
                        &mut coord_index,
                        &mut compressed_index,
                        &mut raw_index,
                    );
                    updated.push(IndexUpdate {
                        path: existing.path.clone(),
                        previous,
                        next: existing.clone(),
                    });
                }
                None => {
                    register_indexes(
                        &normalized,
                        &mut coord_index,
                        &mut compressed_index,
                        &mut raw_index,
                    );
                    merged_map.insert(normalized.path.clone(), normalized.clone());
                    added.push(normalized);
                }
            }
        }

        let merged_index = CanonicalIndex::new(merged_map.into_values().collect());

        Ok(MergeReport {
            index: merged_index,
            added,
            updated,
            unchanged,
            conflicts,
        })
    }

    /// Render canonical JSON (UTF-8, key order path/coords/raw_sha256/compressed_sha256/size/platform/compressed, newline terminated).
    pub fn to_canonical_json(&self) -> Result<String, CasError> {
        let mut array = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let mut obj = serde_json::Map::new();
            obj.insert("path".into(), Value::String(entry.path.clone()));
            if !entry.coords.is_empty() {
                obj.insert(
                    "coords".into(),
                    Value::Array(
                        entry
                            .coords
                            .iter()
                            .map(|c| Value::String(c.clone()))
                            .collect(),
                    ),
                );
            }
            if let Some(meta) = &entry.metadata {
                let mut meta_obj = serde_json::Map::new();
                if let Some(filename) = &meta.filename {
                    meta_obj.insert("filename".into(), Value::String(filename.clone()));
                }
                if let Some(kind) = &meta.kind {
                    meta_obj.insert("kind".into(), Value::String(kind.clone()));
                }
                if !meta_obj.is_empty() {
                    obj.insert("metadata".into(), Value::Object(meta_obj));
                }
            }
            obj.insert("raw_sha256".into(), Value::String(entry.raw_sha256.clone()));
            if let Some(digest) = &entry.compressed_sha256 {
                obj.insert("compressed_sha256".into(), Value::String(digest.clone()));
            }
            if let Some(size) = entry.size {
                obj.insert("size".into(), Value::Number(size.into()));
            }
            if !entry.platform.is_empty() {
                obj.insert(
                    "platform".into(),
                    Value::Array(
                        entry
                            .platform
                            .iter()
                            .map(|p| Value::String(p.clone()))
                            .collect(),
                    ),
                );
            }
            if let Some(compressed) = &entry.compressed {
                let mut comp = serde_json::Map::new();
                comp.insert("alg".into(), Value::String(compressed.alg.clone()));
                if let Some(size) = compressed.size {
                    comp.insert("size".into(), Value::Number(size.into()));
                }
                if let Some(digest) = compressed.digest() {
                    comp.insert("sha256".into(), Value::String(digest.to_string()));
                }
                obj.insert("compressed".into(), Value::Object(comp));
            }
            array.push(Value::Object(obj));
        }
        let json = serde_json::to_string(&Value::Array(array))?;
        Ok(format!("{json}\n"))
    }
}

fn has_inconsistent_raw_or_size(group: &[&IndexEntry]) -> bool {
    let mut seen = HashSet::new();
    for entry in group {
        seen.insert((
            entry.raw_sha256.as_str(),
            entry.size,
            entry.platform.clone(),
        ));
    }
    seen.len() > 1
}

fn has_inconsistent_compressed(group: &[&IndexEntry]) -> bool {
    let mut seen = HashSet::new();
    for entry in group {
        let digest = entry.compressed_sha256.as_deref().unwrap_or("");
        seen.insert((digest.to_string(), entry.size, entry.platform.clone()));
    }
    seen.len() > 1
}

fn register_indexes(
    entry: &IndexEntry,
    coord_index: &mut HashMap<String, (Option<String>, String)>,
    compressed_index: &mut HashMap<String, (String, Option<u64>, String)>,
    raw_index: &mut HashMap<String, (Option<String>, Option<u64>, String)>,
) {
    for coord in &entry.coords {
        coord_index.insert(
            coord.clone(),
            (entry.compressed_sha256.clone(), entry.path.clone()),
        );
    }
    if let Some(digest) = entry.compressed_sha256.clone() {
        compressed_index.insert(
            digest,
            (entry.raw_sha256.clone(), entry.size, entry.path.clone()),
        );
    }
    raw_index.insert(
        entry.raw_sha256.clone(),
        (
            entry.compressed_sha256.clone(),
            entry.size,
            entry.path.clone(),
        ),
    );
}

fn unregister_indexes(
    entry: &IndexEntry,
    coord_index: &mut HashMap<String, (Option<String>, String)>,
    compressed_index: &mut HashMap<String, (String, Option<u64>, String)>,
    raw_index: &mut HashMap<String, (Option<String>, Option<u64>, String)>,
) {
    for coord in &entry.coords {
        if let Some((_, path)) = coord_index.get(coord) {
            if path == &entry.path {
                coord_index.remove(coord);
            }
        }
    }
    if let Some(digest) = entry.compressed_sha256.as_ref() {
        if let Some((_, _, path)) = compressed_index.get(digest) {
            if path == &entry.path {
                compressed_index.remove(digest);
            }
        }
    }
    if let Some((_, _, path)) = raw_index.get(&entry.raw_sha256) {
        if path == &entry.path {
            raw_index.remove(&entry.raw_sha256);
        }
    }
}

fn find_coord_conflict(
    entry: &IndexEntry,
    coord_index: &HashMap<String, (Option<String>, String)>,
    merged_map: &BTreeMap<String, IndexEntry>,
) -> Option<MergeConflictBuilder> {
    for coord in &entry.coords {
        if let Some((existing_digest, owner_path)) = coord_index.get(coord) {
            let matches = match (&entry.compressed_sha256, existing_digest) {
                (Some(new_digest), Some(existing)) => new_digest == existing,
                (None, None) => true,
                _ => false,
            };
            if !matches {
                if let Some(existing_entry) = merged_map.get(owner_path) {
                    return Some(MergeConflictBuilder {
                        kind: MergeConflictKind::Coord,
                        key: coord.clone(),
                        reason: format!(
                            "coordinate '{}' already mapped to path '{}'",
                            coord, owner_path
                        ),
                        existing: existing_entry.clone(),
                    });
                }
            }
        }
    }
    None
}

fn find_compressed_conflict(
    entry: &IndexEntry,
    compressed_index: &HashMap<String, (String, Option<u64>, String)>,
    merged_map: &BTreeMap<String, IndexEntry>,
) -> Option<MergeConflictBuilder> {
    if let Some(digest) = entry.compressed_sha256.as_ref() {
        if let Some((existing_raw, existing_size, owner_path)) = compressed_index.get(digest) {
            if existing_raw != &entry.raw_sha256 || existing_size != &entry.size {
                if let Some(existing_entry) = merged_map.get(owner_path) {
                    return Some(MergeConflictBuilder {
                        kind: MergeConflictKind::CompressedSha256,
                        key: digest.clone(),
                        reason: format!(
                            "compressed digest '{}' already mapped to path '{}'",
                            digest, owner_path
                        ),
                        existing: existing_entry.clone(),
                    });
                }
            }
        }
    }
    None
}

fn find_raw_conflict(
    entry: &IndexEntry,
    raw_index: &HashMap<String, (Option<String>, Option<u64>, String)>,
    merged_map: &BTreeMap<String, IndexEntry>,
) -> Option<MergeConflictBuilder> {
    if let Some((existing_digest, _existing_size, owner_path)) = raw_index.get(&entry.raw_sha256) {
        let matches = match (&entry.compressed_sha256, existing_digest) {
            (Some(new_digest), Some(existing)) => new_digest == existing,
            (None, None) => true,
            (None, Some(_)) | (Some(_), None) => false,
        };
        if !matches {
            if let Some(existing_entry) = merged_map.get(owner_path) {
                return Some(MergeConflictBuilder {
                    kind: MergeConflictKind::RawSha256,
                    key: entry.raw_sha256.clone(),
                    reason: format!(
                        "raw digest '{}' already associated with path '{}'",
                        entry.raw_sha256, owner_path
                    ),
                    existing: existing_entry.clone(),
                });
            }
        }
    }
    None
}

#[derive(Debug)]
pub struct MergeConflictBuilder {
    kind: MergeConflictKind,
    key: String,
    reason: String,
    existing: IndexEntry,
}

impl MergeConflictBuilder {
    fn with_incoming(self, incoming: IndexEntry) -> MergeConflict {
        MergeConflict {
            kind: self.kind,
            key: self.key,
            existing: self.existing,
            incoming,
            reason: self.reason,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MergeReport {
    pub index: CanonicalIndex,
    pub added: Vec<IndexEntry>,
    pub updated: Vec<IndexUpdate>,
    pub unchanged: Vec<IndexEntry>,
    pub conflicts: Vec<MergeConflict>,
}

#[derive(Debug, Clone)]
pub struct IndexUpdate {
    pub path: String,
    pub previous: IndexEntry,
    pub next: IndexEntry,
}

#[derive(Debug, Clone)]
pub struct MergeConflict {
    pub kind: MergeConflictKind,
    pub key: String,
    pub existing: IndexEntry,
    pub incoming: IndexEntry,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeConflictKind {
    Path,
    Coord,
    CompressedSha256,
    RawSha256,
}

#[derive(Debug, Clone)]
pub struct IndexDiff<'a> {
    pub added: Vec<&'a IndexEntry>,
    pub removed: Vec<&'a IndexEntry>,
    pub changed: Vec<IndexChange<'a>>,
}

#[derive(Debug, Clone)]
pub struct IndexChange<'a> {
    pub path: &'a str,
    pub previous: &'a IndexEntry,
    pub next: &'a IndexEntry,
}

#[derive(Debug, Clone)]
pub struct Duplicate<'a> {
    pub kind: DuplicateKind<'a>,
    pub entries: Vec<&'a IndexEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateKind<'a> {
    Path(&'a str),
    Coord(&'a str),
    CompressedSha256(&'a str),
    RawSha256(&'a str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_index_roundtrip_sorts_entries_and_renders_new_fields() {
        let json = r#"
        [
            {
                "path":"b/file",
                "coords":["pkg:python/numpy@1.0.0"],
                "raw_sha256":"bb",
                "compressed_sha256":"ccd",
                "compressed":{"alg":"zstd","size":10,"sha256":"ccd"},
                "size":123,
                "platform":["linux-x86_64"]
            },
            {
                "path":"a/file",
                "sha256":"aa",
                "compressed":{"alg":"zstd","sha256":"dda"}
            }
        ]
        "#;
        let index = CanonicalIndex::from_json_str(json).unwrap();
        assert_eq!(index.entries()[0].path, "a/file");
        assert_eq!(index.entries()[1].path, "b/file");
        assert_eq!(index.entries()[0].compressed_sha256.as_deref(), Some("dda"));
        let rendered = index.to_canonical_json().unwrap();
        assert!(rendered.starts_with("["));
        assert!(rendered.contains("\"path\":\"a/file\""));
        assert!(rendered.contains("\"raw_sha256\":\"aa\""));
        assert!(rendered.ends_with('\n'));
    }

    #[test]
    fn detects_duplicate_paths() {
        let entries = vec![
            IndexEntry {
                path: "dup".into(),
                raw_sha256: "raw1".into(),
                compressed_sha256: Some("comp1".into()),
                compressed: Some(CompressedEntry {
                    alg: "zstd".into(),
                    size: None,
                    digest: None,
                }),
                ..Default::default()
            },
            IndexEntry {
                path: "dup".into(),
                raw_sha256: "raw1".into(),
                compressed_sha256: Some("comp1".into()),
                compressed: Some(CompressedEntry {
                    alg: "zstd".into(),
                    size: None,
                    digest: None,
                }),
                ..Default::default()
            },
        ];
        let index = CanonicalIndex::new(entries);
        let duplicates = index.duplicates();
        assert_eq!(duplicates.len(), 1);
        assert!(matches!(duplicates[0].kind, DuplicateKind::Path("dup")));
    }

    #[test]
    fn merge_adds_and_updates_entries() {
        let base = CanonicalIndex::from_entries(vec![IndexEntry {
            path: "a".into(),
            raw_sha256: "raw1".into(),
            compressed_sha256: Some("comp1".into()),
            size: Some(1),
            compressed: Some(CompressedEntry {
                alg: "zstd".into(),
                size: None,
                digest: None,
            }),
            ..Default::default()
        }])
        .unwrap();
        let report = base
            .merge(vec![
                IndexEntry {
                    path: "a".into(),
                    raw_sha256: "raw2".into(),
                    compressed_sha256: Some("comp2".into()),
                    size: Some(2),
                    compressed: Some(CompressedEntry {
                        alg: "zstd".into(),
                        size: None,
                        digest: None,
                    }),
                    ..Default::default()
                },
                IndexEntry {
                    path: "b".into(),
                    raw_sha256: "raw3".into(),
                    compressed_sha256: Some("comp3".into()),
                    compressed: Some(CompressedEntry {
                        alg: "zstd".into(),
                        size: None,
                        digest: None,
                    }),
                    ..Default::default()
                },
            ])
            .unwrap();
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.updated.len(), 1);
        assert!(report.conflicts.is_empty());
        let merged = report.index;
        assert_eq!(merged.entries().len(), 2);
        assert_eq!(merged.entries()[0].path, "a");
        assert_eq!(merged.entries()[0].raw_sha256, "raw2");
    }

    #[test]
    fn merge_reports_conflicting_compressed_digest() {
        let base = CanonicalIndex::from_entries(vec![IndexEntry {
            path: "a".into(),
            raw_sha256: "raw1".into(),
            compressed_sha256: Some("comp1".into()),
            size: Some(10),
            compressed: Some(CompressedEntry {
                alg: "zstd".into(),
                size: None,
                digest: None,
            }),
            ..Default::default()
        }])
        .unwrap();
        let report = base
            .merge(vec![IndexEntry {
                path: "b".into(),
                raw_sha256: "raw2".into(),
                compressed_sha256: Some("comp1".into()),
                size: Some(5),
                compressed: Some(CompressedEntry {
                    alg: "zstd".into(),
                    size: None,
                    digest: None,
                }),
                ..Default::default()
            }])
            .unwrap();
        assert_eq!(report.conflicts.len(), 1);
        assert!(matches!(
            report.conflicts[0].kind,
            MergeConflictKind::CompressedSha256
        ));
    }

    #[test]
    fn normalize_rejects_excessive_ratio_metadata() {
        let entry = IndexEntry {
            path: "sha256-deadbeef".into(),
            raw_sha256: "a".repeat(64),
            compressed_sha256: Some("b".repeat(64)),
            size: Some(5 * 1024 * 1024 * 1024),
            platform: vec![],
            coords: vec![],
            compressed: Some(CompressedEntry {
                alg: "zstd".into(),
                size: Some(100 * 1024 * 1024),
                digest: Some("b".repeat(64)),
            }),
            metadata: None,
        };
        let err = entry.normalize().expect_err("ratio limit should trigger");
        assert!(matches!(err, CasError::CompressionRatioExceeded { .. }));
    }

    #[test]
    fn diff_detects_changes() {
        let left = CanonicalIndex::from_entries(vec![IndexEntry {
            path: "a".into(),
            raw_sha256: "raw1".into(),
            compressed_sha256: Some("comp1".into()),
            compressed: Some(CompressedEntry {
                alg: "zstd".into(),
                size: None,
                digest: None,
            }),
            ..Default::default()
        }])
        .unwrap();
        let right = CanonicalIndex::from_entries(vec![
            IndexEntry {
                path: "a".into(),
                raw_sha256: "raw2".into(),
                compressed_sha256: Some("comp2".into()),
                compressed: Some(CompressedEntry {
                    alg: "zstd".into(),
                    size: None,
                    digest: None,
                }),
                ..Default::default()
            },
            IndexEntry {
                path: "b".into(),
                raw_sha256: "raw3".into(),
                compressed_sha256: Some("comp3".into()),
                compressed: Some(CompressedEntry {
                    alg: "zstd".into(),
                    size: None,
                    digest: None,
                }),
                ..Default::default()
            },
        ])
        .unwrap();
        let diff = left.diff(&right);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.removed.len(), 0);
        assert_eq!(diff.changed[0].path, "a");
    }

    #[test]
    fn deserialize_legacy_fields() {
        let raw = json!([{
            "path": "legacy.bin",
            "sha256": "raw-legacy",
            "compressed": {
                "alg": "zstd",
                "sha256": "comp-legacy"
            }
        }]);
        let data = serde_json::to_string(&raw).unwrap();
        let index = CanonicalIndex::from_json_str(&data).unwrap();
        assert_eq!(index.entries().len(), 1);
        let entry = &index.entries()[0];
        assert_eq!(entry.raw_sha256, "raw-legacy");
        assert_eq!(entry.compressed_sha256.as_deref(), Some("comp-legacy"));
    }
}
