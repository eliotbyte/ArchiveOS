use std::fs;
use std::path::{Path, PathBuf};

use archiveos_contract::{
    VaultError, VaultMetadata, ARCHIVEOS_DIR, BLOBS_DIR, DB_FILE, LAYOUT_VERSION, STAGING_DIR,
    VAULT_JSON, WORKER_CACHE_DIR, WORKER_COOKIES_DIR, WORKER_THUMBNAIL, WORKER_YTDLP, WORKERS_DIR,
};

pub fn is_dir_empty(path: &Path) -> Result<bool, VaultError> {
    if !path.is_dir() {
        return Ok(false);
    }
    Ok(fs::read_dir(path)?.next().is_none())
}

pub fn validate_layout(root: &Path) -> Result<VaultMetadata, VaultError> {
    if !root.is_dir() {
        return Err(VaultError::NotFound);
    }

    let archiveos = root.join(ARCHIVEOS_DIR);
    let vault_json = archiveos.join(VAULT_JSON);
    let db_file = archiveos.join(DB_FILE);
    let blobs = root.join(BLOBS_DIR);
    let staging = root.join(STAGING_DIR);

    for (label, path, must_be_dir) in [
        ("vault.json", vault_json.clone(), false),
        ("db.sqlite", db_file, false),
        ("blobs/", blobs.clone(), true),
        ("staging/", staging, true),
    ] {
        if !path.exists() {
            return Err(VaultError::InvalidLayout {
                detail: format!("missing {label}"),
            });
        }
        if must_be_dir && !path.is_dir() {
            return Err(VaultError::InvalidLayout {
                detail: format!("{label} is not a directory"),
            });
        }
    }

    let contents = fs::read_to_string(&vault_json)?;
    let metadata: VaultMetadata = serde_json::from_str(&contents)?;

    if metadata.layout_version != LAYOUT_VERSION {
        return Err(VaultError::InvalidLayout {
            detail: format!(
                "unsupported layout_version {} (expected {LAYOUT_VERSION})",
                metadata.layout_version
            ),
        });
    }

    Ok(metadata)
}

pub fn blob_path(root: &Path, hash: &str, ext: &str) -> Result<PathBuf, VaultError> {
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(VaultError::InvalidLayout {
            detail: "hash must be 64 hex characters".into(),
        });
    }
    let ext = if ext.starts_with('.') {
        ext.to_string()
    } else {
        format!(".{ext}")
    };
    Ok(root
        .join(BLOBS_DIR)
        .join(&hash[0..2])
        .join(&hash[2..4])
        .join(format!("{hash}{ext}")))
}

pub fn find_blob_path(
    root: &Path,
    hash: &str,
    ext: Option<&str>,
    mime: Option<&str>,
) -> Result<PathBuf, VaultError> {
    let mut candidates = Vec::new();
    if let Some(ext) = ext {
        let ext = ext.trim_start_matches('.');
        if !ext.is_empty() {
            candidates.push(ext.to_string());
        }
    }
    if let Some(mime) = mime.and_then(mime_extension) {
        if !candidates.iter().any(|candidate| candidate == &mime) {
            candidates.push(mime);
        }
    }

    for ext in &candidates {
        let path = blob_path(root, hash, ext)?;
        if path.is_file() {
            return Ok(path);
        }
    }

    if hash.len() != 64 {
        return Err(VaultError::NotFound);
    }

    let shard = root
        .join(BLOBS_DIR)
        .join(&hash[0..2])
        .join(&hash[2..4]);
    if !shard.is_dir() {
        return Err(VaultError::NotFound);
    }

    let prefix = format!("{hash}.");
    for entry in fs::read_dir(shard).map_err(VaultError::Io)? {
        let entry = entry.map_err(VaultError::Io)?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == hash || name.starts_with(&prefix) {
            return Ok(path);
        }
    }

    Err(VaultError::NotFound)
}

fn mime_extension(mime: &str) -> Option<String> {
    mime_guess::get_mime_extensions_str(mime)
        .and_then(|extensions| extensions.first().copied())
        .map(str::to_string)
}

pub fn staging_job_path(root: &Path, job_id: &str) -> PathBuf {
    root.join(STAGING_DIR).join(job_id)
}

pub fn worker_dir(root: &Path, worker: &str) -> PathBuf {
    root.join(WORKERS_DIR).join(worker)
}

pub fn worker_cache_dir(root: &Path, worker: &str) -> PathBuf {
    worker_dir(root, worker).join(WORKER_CACHE_DIR)
}

pub fn worker_cookies_dir(root: &Path, worker: &str) -> PathBuf {
    worker_dir(root, worker).join(WORKER_COOKIES_DIR)
}

pub fn ytdlp_worker_dir(root: &Path) -> PathBuf {
    worker_dir(root, WORKER_YTDLP)
}

pub fn thumbnail_worker_dir(root: &Path) -> PathBuf {
    worker_dir(root, WORKER_THUMBNAIL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use archiveos_contract::VaultMetadata;
    use chrono::TimeZone;
    use std::path::Path;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn staging_job_path_joins_job_id() {
        let root = Path::new("/vault");
        assert_eq!(
            staging_job_path(root, "job-1"),
            root.join("staging/job-1")
        );
    }

    #[test]
    fn worker_dirs_use_workers_namespace() {
        let root = Path::new("/vault");
        assert_eq!(
            ytdlp_worker_dir(root),
            root.join("workers/ytdlp")
        );
        assert_eq!(
            worker_cookies_dir(root, WORKER_YTDLP),
            root.join("workers/ytdlp/cookies")
        );
        assert_eq!(
            thumbnail_worker_dir(root),
            root.join("workers/thumbnail")
        );
    }

    #[test]
    fn find_blob_path_resolves_legacy_assets_without_ext() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(BLOBS_DIR)).unwrap();

        let hash = "6200e5549af151619012e76db6b308a6ab3fd730c47491fc5ff99646f352d4c8";
        let blob = root.join("blobs/62/00").join(format!("{hash}.webp"));
        fs::create_dir_all(blob.parent().unwrap()).unwrap();
        fs::write(&blob, b"webp").unwrap();

        let resolved = find_blob_path(root, hash, None, Some("image/webp")).unwrap();
        assert_eq!(resolved, blob);
    }

    #[test]
    fn blob_path_shards_correctly() {
        let root = Path::new("/vault");
        let hash = "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234";
        let path = blob_path(root, hash, ".mp4").unwrap();
        assert_eq!(
            path,
            root.join("blobs/ab/cd/abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234.mp4")
        );
    }

    #[test]
    fn blob_path_rejects_invalid_hash() {
        let root = Path::new("/vault");
        assert!(blob_path(root, "tooshort", ".mp4").is_err());
        assert!(blob_path(
            root,
            "gggg1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234",
            ".mp4"
        )
        .is_err());
    }

    #[test]
    fn validate_layout_accepts_valid_vault() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let archiveos = root.join(ARCHIVEOS_DIR);
        fs::create_dir_all(&archiveos).unwrap();
        fs::create_dir_all(root.join(BLOBS_DIR)).unwrap();
        fs::create_dir_all(root.join(STAGING_DIR)).unwrap();
        fs::write(archiveos.join(DB_FILE), []).unwrap();

        let metadata = VaultMetadata::new(
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            chrono::Utc.with_ymd_and_hms(2026, 6, 20, 12, 0, 0).unwrap(),
        );
        fs::write(
            archiveos.join(VAULT_JSON),
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        let loaded = validate_layout(root).unwrap();
        assert_eq!(loaded.id, metadata.id);
    }

    #[test]
    fn validate_layout_fails_on_missing_blobs() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let archiveos = root.join(ARCHIVEOS_DIR);
        fs::create_dir_all(&archiveos).unwrap();
        fs::create_dir_all(root.join(STAGING_DIR)).unwrap();
        fs::write(archiveos.join(DB_FILE), []).unwrap();

        let metadata = VaultMetadata::new(Uuid::new_v4(), chrono::Utc::now());
        fs::write(
            archiveos.join(VAULT_JSON),
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        assert!(matches!(
            validate_layout(root),
            Err(VaultError::InvalidLayout { .. })
        ));
    }
}
