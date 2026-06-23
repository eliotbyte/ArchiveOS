use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use archiveos_contract::VaultError;
use sha2::{Digest, Sha256};

use crate::layout::blob_path;

pub mod gc;
pub mod refcount;

pub use gc::{remove_blob_if_unreferenced, sweep_unreferenced_blobs, BlobGcOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CasStoreResult {
    pub content_hash: String,
    pub blob_path: PathBuf,
    pub deduped: bool,
    pub size: u64,
}

pub fn hash_file(path: &Path) -> Result<String, VaultError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn store_managed(
    vault_root: &Path,
    staging_file: &Path,
    ext: &str,
) -> Result<CasStoreResult, VaultError> {
    if !staging_file.is_file() {
        return Err(VaultError::InvalidLayout {
            detail: format!("staging file not found: {}", staging_file.display()),
        });
    }

    let size = fs::metadata(staging_file)?.len();
    let content_hash = hash_file(staging_file)?;
    let dest = blob_path(vault_root, &content_hash, ext)?;

    if dest.exists() {
        fs::remove_file(staging_file)?;
        return Ok(CasStoreResult {
            content_hash,
            blob_path: dest,
            deduped: true,
            size,
        });
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::rename(staging_file, &dest).map_err(|err| {
        if err.raw_os_error() == Some(18) {
            // EXDEV: cross-device link — should not happen within vault volume
            VaultError::InvalidLayout {
                detail: format!(
                    "atomic rename failed across filesystems: {} -> {}",
                    staging_file.display(),
                    dest.display()
                ),
            }
        } else {
            VaultError::Io(err)
        }
    })?;

    Ok(CasStoreResult {
        content_hash,
        blob_path: dest,
        deduped: false,
        size,
    })
}

pub fn blob_exists(vault_root: &Path, hash: &str, ext: &str) -> Result<bool, VaultError> {
    Ok(blob_path(vault_root, hash, ext)?.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn hash_file_returns_sha256_hex() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.bin");
        std::fs::write(&path, b"hello").unwrap();
        let hash = hash_file(&path).unwrap();
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn store_moves_staging_file_to_sharded_blob_path() {
        let vault_root = tempdir().unwrap();
        std::fs::create_dir_all(vault_root.path().join("blobs")).unwrap();

        let staging = vault_root.path().join("staging/job1/files/video.mp4");
        std::fs::create_dir_all(staging.parent().unwrap()).unwrap();
        std::fs::write(&staging, b"video-bytes").unwrap();

        let result = store_managed(vault_root.path(), &staging, ".mp4").unwrap();
        assert!(!result.deduped);
        assert!(result.blob_path.exists());
        assert!(!staging.exists());
        assert_eq!(
            result.blob_path,
            vault_root.path().join(format!(
                "blobs/{}/{}/{}.mp4",
                &result.content_hash[0..2],
                &result.content_hash[2..4],
                result.content_hash
            ))
        );
    }

    #[test]
    fn store_dedup_deletes_staging_when_blob_exists() {
        let vault_root = tempdir().unwrap();
        std::fs::create_dir_all(vault_root.path().join("blobs")).unwrap();

        let staging1 = vault_root.path().join("staging/job1/files/a.mp4");
        std::fs::create_dir_all(staging1.parent().unwrap()).unwrap();
        std::fs::write(&staging1, b"same-content").unwrap();
        let first = store_managed(vault_root.path(), &staging1, ".mp4").unwrap();
        assert!(!first.deduped);

        let staging2 = vault_root.path().join("staging/job2/files/b.mp4");
        std::fs::create_dir_all(staging2.parent().unwrap()).unwrap();
        std::fs::write(&staging2, b"same-content").unwrap();
        let second = store_managed(vault_root.path(), &staging2, ".mp4").unwrap();

        assert!(second.deduped);
        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(first.blob_path, second.blob_path);
        assert!(!staging2.exists());
        assert_eq!(
            std::fs::read(&first.blob_path).unwrap(),
            b"same-content".as_slice()
        );
    }
}
