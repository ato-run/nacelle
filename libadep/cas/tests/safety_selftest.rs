use libadep_cas::safety::ensure_archive_member_safe;
use libadep_cas::{CanonicalIndex, CasError, CompressedEntry, IndexEntry};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;
use zip::write::FileOptions;
use zip::ZipWriter;

#[test]
fn zip_slip_fixture_is_blocked() {
    let temp = TempDir::new().unwrap();
    let zip_path = temp.path().join("slip.zip");
    let file = File::create(&zip_path).unwrap();
    let mut writer = ZipWriter::new(file);
    writer
        .start_file("pkg/data.txt", FileOptions::default())
        .unwrap();
    writer.write_all(b"ok").unwrap();
    writer
        .start_file("../escape.sh", FileOptions::default())
        .unwrap();
    writer.write_all(b"boom").unwrap();
    writer.finish().unwrap();

    let mut archive = zip::ZipArchive::new(File::open(&zip_path).unwrap()).unwrap();
    let mut detected = false;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).unwrap();
        match ensure_archive_member_safe(Path::new(entry.name())) {
            Ok(_) => {}
            Err(CasError::ZipSlip { .. }) => {
                detected = true;
            }
            Err(other) => panic!("unexpected error: {:?}", other),
        }
    }
    assert!(detected, "zip slip path should be rejected");
}

#[test]
fn canonical_index_rejects_large_ratio_fixture() {
    let oversized = IndexEntry {
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
    let err = CanonicalIndex::from_entries(vec![oversized]).expect_err("ratio limit should fail");
    assert!(matches!(err, CasError::CompressionRatioExceeded { .. }));
}
