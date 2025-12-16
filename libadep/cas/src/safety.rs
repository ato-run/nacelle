use crate::CasError;
use std::path::{Component, Path, PathBuf};

pub const MAX_COMPRESSION_RATIO: f64 = 20.0;

/// Ensure the ratio of `raw` bytes to `compressed` bytes does not exceed `MAX_COMPRESSION_RATIO`.
pub fn enforce_compression_ratio(raw: u64, compressed: u64) -> Result<(), CasError> {
    if raw == 0 {
        return Ok(());
    }
    if compressed == 0 {
        return Err(CasError::CompressionRatioExceeded {
            raw,
            compressed,
            limit: MAX_COMPRESSION_RATIO,
        });
    }
    let ratio = raw as f64 / compressed as f64;
    if ratio > MAX_COMPRESSION_RATIO {
        return Err(CasError::CompressionRatioExceeded {
            raw,
            compressed,
            limit: MAX_COMPRESSION_RATIO,
        });
    }
    Ok(())
}

/// Validate that an archive entry path is safe to materialize under the provided base directory.
///
/// - Rejects absolute paths, drive prefixes, and `..` components.
/// - Normalizes redundant `.` components.
pub fn ensure_archive_member_safe(entry: &Path) -> Result<(), CasError> {
    if entry.as_os_str().is_empty() {
        return Err(CasError::ZipSlip {
            entry: String::from("<empty>"),
        });
    }
    for component in entry.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(CasError::ZipSlip {
                    entry: entry.display().to_string(),
                })
            }
            _ => {}
        }
    }
    Ok(())
}

/// Resolve an archive member path under `base`, ensuring Zip Slip style traversal is not possible.
pub fn resolve_archive_member(base: &Path, entry: &Path) -> Result<PathBuf, CasError> {
    ensure_archive_member_safe(entry)?;
    let mut normalized = PathBuf::new();
    for component in entry.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(part) => normalized.push(part),
            _ => unreachable!("unsafe components filtered by ensure_archive_member_safe"),
        }
    }
    Ok(base.join(normalized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::MAX_COMPRESSION_RATIO;

    #[test]
    fn enforce_ratio_accepts_within_limit() {
        enforce_compression_ratio(2000, 200).expect("ratio 10x should pass");
    }

    #[test]
    fn enforce_ratio_rejects_zero_compressed() {
        let err = enforce_compression_ratio(1, 0).expect_err("zero compressed must fail");
        match err {
            CasError::CompressionRatioExceeded {
                raw,
                compressed,
                limit,
            } => {
                assert_eq!(raw, 1);
                assert_eq!(compressed, 0);
                assert_eq!(limit, MAX_COMPRESSION_RATIO);
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn enforce_ratio_rejects_over_limit() {
        let err = enforce_compression_ratio(1024 * 1024, 1000).expect_err("ratio over 20x");
        assert!(matches!(err, CasError::CompressionRatioExceeded { .. }));
    }

    #[test]
    fn zip_slip_rejected_for_parent_dir() {
        let err =
            ensure_archive_member_safe(Path::new("../evil")).expect_err("parent dir must fail");
        assert!(matches!(err, CasError::ZipSlip { .. }));
    }

    #[test]
    fn zip_slip_accepts_normal_path() {
        ensure_archive_member_safe(Path::new("package/METADATA"))
            .expect("normal archive path should be accepted");
    }

    #[test]
    fn resolve_member_normalizes_curdir() {
        let resolved =
            resolve_archive_member(Path::new("/tmp/cas"), Path::new("./pkg/./data.txt")).unwrap();
        assert_eq!(resolved, PathBuf::from("/tmp/cas/pkg/data.txt"));
    }
}
