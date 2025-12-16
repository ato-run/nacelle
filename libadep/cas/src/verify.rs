#![allow(dead_code)]

use crate::{safety, CasError};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, ErrorKind, Read};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CompressedHash {
    pub alg: String,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub verified_bytes: u64,
}

#[derive(Debug, Default)]
pub struct Verifier;

impl Verifier {
    pub fn new() -> Self {
        Self
    }

    pub fn verify(
        &self,
        compressed: Option<CompressedHash>,
        expected_raw_sha256: &str,
        file_path: &Path,
    ) -> Result<VerificationResult, CasError> {
        if let Some(info) = compressed {
            match info.alg.as_str() {
                "zstd" => return self.verify_zstd(&info, expected_raw_sha256, file_path),
                other => return Err(CasError::UnsupportedCompression(other.to_string())),
            }
        }
        self.verify_raw(expected_raw_sha256, file_path)
    }

    fn verify_raw(
        &self,
        expected_sha256: &str,
        file_path: &Path,
    ) -> Result<VerificationResult, CasError> {
        let file = File::open(file_path)?;
        let (hash, bytes) = hash_reader(BufReader::new(file))?;
        if hash != expected_sha256 {
            return Err(CasError::HashMismatch);
        }
        Ok(VerificationResult {
            verified_bytes: bytes,
        })
    }

    fn verify_zstd(
        &self,
        info: &CompressedHash,
        expected_raw_sha256: &str,
        file_path: &Path,
    ) -> Result<VerificationResult, CasError> {
        let (compressed_hash, compressed_bytes) = {
            let file = File::open(file_path)?;
            hash_reader(BufReader::new(file))?
        };
        if compressed_hash != info.sha256 {
            return Err(CasError::CompressedHashMismatch(info.alg.clone()));
        }

        let file = File::open(file_path)?;
        let decoder = zstd::Decoder::new(BufReader::new(file))
            .map_err(|err| CasError::Decompression(err.to_string()))?;

        let (raw_hash, verified_bytes) = match hash_reader(decoder) {
            Ok(result) => result,
            Err(CasError::Io(err))
                if matches!(
                    err.kind(),
                    ErrorKind::InvalidData | ErrorKind::UnexpectedEof
                ) =>
            {
                return Err(CasError::Decompression(err.to_string()))
            }
            Err(err) => return Err(err),
        };
        if raw_hash != expected_raw_sha256 {
            return Err(CasError::HashMismatch);
        }
        safety::enforce_compression_ratio(verified_bytes, compressed_bytes)?;
        Ok(VerificationResult { verified_bytes })
    }
}

fn hash_reader(mut reader: impl Read) -> Result<(String, u64), CasError> {
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    let mut total = 0u64;
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.update(&buf[..read]);
    }
    Ok((hex::encode(hasher.finalize()), total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use zstd::stream::encode_all;

    #[test]
    fn verify_raw_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "hello raw").unwrap();
        file.flush().unwrap();
        let expected = {
            let (hash, _) = hash_reader(File::open(file.path()).unwrap()).unwrap();
            hash
        };
        let verifier = Verifier::new();
        let result = verifier.verify(None, &expected, file.path()).unwrap();
        assert!(result.verified_bytes > 0);
    }

    #[test]
    fn verify_zstd_success() {
        let raw = b"hello compressed world";
        let expected_raw = {
            let mut hasher = Sha256::new();
            hasher.update(raw);
            hex::encode(hasher.finalize())
        };

        let compressed = encode_all(&raw[..], 0).unwrap();
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&compressed).unwrap();
        file.flush().unwrap();

        let compressed_sha = {
            let (hash, _) = hash_reader(File::open(file.path()).unwrap()).unwrap();
            hash
        };

        let verifier = Verifier::new();
        let result = verifier
            .verify(
                Some(CompressedHash {
                    alg: "zstd".into(),
                    sha256: compressed_sha,
                }),
                &expected_raw,
                file.path(),
            )
            .unwrap();
        assert_eq!(result.verified_bytes as usize, raw.len());
    }

    #[test]
    fn verify_zstd_compressed_hash_mismatch() {
        let raw = b"corrupted compressed hash";
        let expected_raw = {
            let mut hasher = Sha256::new();
            hasher.update(raw);
            hex::encode(hasher.finalize())
        };

        let compressed = encode_all(&raw[..], 0).unwrap();
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&compressed).unwrap();
        file.flush().unwrap();

        let verifier = Verifier::new();
        let err = verifier
            .verify(
                Some(CompressedHash {
                    alg: "zstd".into(),
                    sha256: "deadbeef".into(),
                }),
                &expected_raw,
                file.path(),
            )
            .unwrap_err();
        assert!(matches!(err, CasError::CompressedHashMismatch(_)));
    }

    #[test]
    fn verify_zstd_raw_hash_mismatch() {
        let raw = b"raw mismatch payload";
        let compressed = encode_all(&raw[..], 0).unwrap();
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&compressed).unwrap();
        file.flush().unwrap();

        let compressed_sha = {
            let (hash, _) = hash_reader(File::open(file.path()).unwrap()).unwrap();
            hash
        };
        let verifier = Verifier::new();
        let err = verifier
            .verify(
                Some(CompressedHash {
                    alg: "zstd".into(),
                    sha256: compressed_sha,
                }),
                "deadbeef",
                file.path(),
            )
            .unwrap_err();
        assert!(matches!(err, CasError::HashMismatch));
    }

    #[test]
    fn verify_zstd_decompression_failure() {
        let raw = b"incomplete frame triggers decoder error";
        let compressed_full = encode_all(&raw[..], 0).unwrap();
        let mut truncated = compressed_full.clone();
        truncated.truncate(std::cmp::max(1, truncated.len() / 2));

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&truncated).unwrap();
        file.flush().unwrap();
        let compressed_sha = {
            let (hash, _) = hash_reader(File::open(file.path()).unwrap()).unwrap();
            hash
        };

        let verifier = Verifier::new();
        let err = verifier
            .verify(
                Some(CompressedHash {
                    alg: "zstd".into(),
                    sha256: compressed_sha,
                }),
                "deadbeef",
                file.path(),
            )
            .unwrap_err();
        assert!(
            matches!(err, CasError::Decompression(_)),
            "unexpected error variant: {:?}",
            err
        );
    }

    #[test]
    fn verify_zstd_rejects_excessive_ratio() {
        let raw = vec![0u8; 256 * 1024];
        let expected_raw = {
            let mut hasher = Sha256::new();
            hasher.update(&raw);
            hex::encode(hasher.finalize())
        };
        let compressed = encode_all(&raw[..], 0).unwrap();
        assert!(
            (raw.len() as f64) / (compressed.len() as f64) > crate::safety::MAX_COMPRESSION_RATIO,
            "fixture should exceed ratio limit"
        );

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&compressed).unwrap();
        file.flush().unwrap();

        let compressed_sha = {
            let (hash, _) = hash_reader(File::open(file.path()).unwrap()).unwrap();
            hash
        };

        let verifier = Verifier::new();
        let err = verifier
            .verify(
                Some(CompressedHash {
                    alg: "zstd".into(),
                    sha256: compressed_sha,
                }),
                &expected_raw,
                file.path(),
            )
            .unwrap_err();
        assert!(matches!(err, CasError::CompressionRatioExceeded { .. }));
    }
}
