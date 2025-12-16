use crate::{safety, CasError};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use tempfile::{Builder, NamedTempFile};
use zstd::stream::Encoder;

const DEFAULT_ZSTD_LEVEL: i32 = 3;

#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
    blobs_dir: PathBuf,
    tmp_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobStatus {
    Stored,
    Reused,
}

#[derive(Debug, Clone)]
pub struct StoredBlob {
    pub raw_sha256: String,
    pub raw_size: u64,
    pub compressed_sha256: String,
    pub compressed_size: u64,
    pub path: PathBuf,
    pub status: BlobStatus,
}

#[derive(Debug, Clone, Copy)]
pub struct IngestOptions {
    pub compression_level: i32,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            compression_level: DEFAULT_ZSTD_LEVEL,
        }
    }
}

impl BlobStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, CasError> {
        let root = root.as_ref().to_path_buf();
        let blobs_dir = root.join("blobs");
        let tmp_dir = root.join("tmp");
        fs::create_dir_all(&blobs_dir)?;
        fs::create_dir_all(&tmp_dir)?;
        Ok(Self {
            root,
            blobs_dir,
            tmp_dir,
        })
    }

    pub fn ingest_path(
        &self,
        source: &Path,
        options: Option<IngestOptions>,
    ) -> Result<StoredBlob, CasError> {
        let opts = options.unwrap_or_default();
        let file = fs::File::open(source)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(CasError::InvalidIndex(format!(
                "source {} is not a regular file",
                source.display()
            )));
        }
        self.ingest_reader(BufReader::new(file), opts)
    }

    pub fn ingest_reader<R: Read>(
        &self,
        mut reader: R,
        options: IngestOptions,
    ) -> Result<StoredBlob, CasError> {
        let mut raw_hasher = Sha256::new();
        let mut raw_size = 0u64;
        let hashing_writer = HashingTempFile::new(&self.tmp_dir)?;
        let mut encoder = Encoder::new(hashing_writer, options.compression_level)
            .map_err(|err| CasError::Io(io::Error::other(err)))?;

        let mut buf = [0u8; 8192];
        loop {
            let read = reader.read(&mut buf)?;
            if read == 0 {
                break;
            }
            raw_hasher.update(&buf[..read]);
            raw_size += read as u64;
            encoder.write_all(&buf[..read])?;
        }
        let raw_sha256 = hex::encode(raw_hasher.finalize());
        let hashing_writer = encoder
            .finish()
            .map_err(|err| CasError::Io(io::Error::other(err)))?;
        let (temp_file, compressed_sha256, compressed_size) = hashing_writer.finalize()?;

        safety::enforce_compression_ratio(raw_size, compressed_size)?;

        let final_path = self.blob_path(&compressed_sha256);
        let status = if final_path.exists() {
            BlobStatus::Reused
        } else {
            match temp_file.persist_noclobber(&final_path) {
                Ok(_) => BlobStatus::Stored,
                Err(err) if err.error.kind() == io::ErrorKind::AlreadyExists => BlobStatus::Reused,
                Err(err) => return Err(CasError::Io(err.error)),
            }
        };

        Ok(StoredBlob {
            raw_sha256,
            raw_size,
            compressed_sha256,
            compressed_size,
            path: final_path,
            status,
        })
    }

    pub fn blob_path(&self, digest: &str) -> PathBuf {
        self.blobs_dir.join(format!("sha256-{digest}"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

struct HashingTempFile {
    inner: NamedTempFile,
    hasher: Sha256,
    written: u64,
}

impl HashingTempFile {
    fn new(dir: &Path) -> Result<Self, CasError> {
        let inner = Builder::new().prefix("blob-").tempfile_in(dir)?;
        Ok(Self {
            inner,
            hasher: Sha256::new(),
            written: 0,
        })
    }

    fn finalize(mut self) -> Result<(NamedTempFile, String, u64), CasError> {
        self.inner.as_file_mut().flush()?;
        let digest = hex::encode(self.hasher.finalize());
        Ok((self.inner, digest, self.written))
    }
}

impl Write for HashingTempFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        if written > 0 {
            self.hasher.update(&buf[..written]);
            self.written += written as u64;
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn ingest_path_stores_and_reuses() {
        let temp = TempDir::new().unwrap();
        let cas_root = temp.path().join("cas");
        let store = BlobStore::open(&cas_root).unwrap();

        let artifact_path = temp.path().join("artifact.bin");
        {
            let mut file = fs::File::create(&artifact_path).unwrap();
            writeln!(file, "hello artifact").unwrap();
        }

        let stored = store.ingest_path(&artifact_path, None).unwrap();
        assert_eq!(stored.status, BlobStatus::Stored);
        assert!(stored.path.exists());

        let reused = store.ingest_path(&artifact_path, None).unwrap();
        assert_eq!(reused.status, BlobStatus::Reused);
        assert_eq!(stored.compressed_sha256, reused.compressed_sha256);
    }

    #[test]
    fn ingest_reader_computes_raw_digest() {
        let temp = TempDir::new().unwrap();
        let store = BlobStore::open(temp.path()).unwrap();
        let data = b"abcdefg";
        let raw_sha = {
            let mut hasher = Sha256::new();
            hasher.update(data);
            hex::encode(hasher.finalize())
        };
        let stored = store
            .ingest_reader(&data[..], IngestOptions::default())
            .unwrap();
        assert_eq!(stored.raw_sha256, raw_sha);
        assert_eq!(stored.status, BlobStatus::Stored);
        assert!(stored.path.exists());
    }

    #[test]
    fn ingest_reader_rejects_excessive_ratio() {
        let temp = TempDir::new().unwrap();
        let store = BlobStore::open(temp.path()).unwrap();
        let data = vec![0u8; 512 * 1024];
        let err = store
            .ingest_reader(&data[..], IngestOptions::default())
            .expect_err("excessive compression ratio should fail");
        assert!(matches!(err, CasError::CompressionRatioExceeded { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn ingest_reader_reports_permission_denied() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let store = BlobStore::open(temp.path()).unwrap();
        let tmp_dir = store.root().join("tmp");
        let perms = fs::metadata(&tmp_dir).unwrap().permissions();
        let mut readonly = perms.clone();
        readonly.set_mode(0o555);
        fs::set_permissions(&tmp_dir, readonly).unwrap();

        let err = store
            .ingest_reader(&[1u8; 64][..], IngestOptions::default())
            .expect_err("permission denied expected");
        assert!(matches!(err, CasError::Io(_)));

        let mut restore = fs::metadata(&tmp_dir).unwrap().permissions();
        restore.set_mode(0o755);
        fs::set_permissions(&tmp_dir, restore).unwrap();
    }
}
