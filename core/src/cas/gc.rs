use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use archiveos_contract::{BlobGcReport, VaultError, BLOBS_DIR};
use rusqlite::Connection;

use super::refcount::collect_pinned_hashes;
use crate::layout::find_blob_path;

#[derive(Debug, Clone)]
pub struct BlobGcOptions {
    pub dry_run: bool,
    pub min_age_secs: u64,
}

pub fn sweep_unreferenced_blobs(
    conn: &Connection,
    vault_root: &Path,
    opts: BlobGcOptions,
) -> Result<BlobGcReport, VaultError> {
    let pinned = collect_pinned_hashes(conn)?;
    let on_disk = list_managed_blob_files(vault_root)?;
    let now = SystemTime::now();
    let mut report = BlobGcReport {
        scanned: u32::try_from(on_disk.len()).unwrap_or(u32::MAX),
        candidates: 0,
        removed: 0,
        bytes_freed: 0,
        errors: Vec::new(),
    };

    for (hash, path) in on_disk {
        if pinned.contains(&hash) {
            continue;
        }
        if !file_older_than(&path, now, opts.min_age_secs)? {
            continue;
        }
        report.candidates += 1;
        let size = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
        if opts.dry_run {
            report.bytes_freed += size;
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => {
                report.removed += 1;
                report.bytes_freed += size;
            }
            Err(err) => report
                .errors
                .push(format!("failed to remove {}: {err}", path.display())),
        }
    }

    Ok(report)
}

pub fn remove_blob_if_unreferenced(
    conn: &Connection,
    vault_root: &Path,
    hash: &str,
    ext: Option<&str>,
    mime: Option<&str>,
) -> Result<bool, VaultError> {
    if super::refcount::blob_reference_count(conn, hash)? > 0 {
        return Ok(false);
    }
    let path = find_blob_path(vault_root, hash, ext, mime)?;
    if path.is_file() {
        fs::remove_file(&path).map_err(VaultError::Io)?;
    }
    Ok(true)
}

fn file_older_than(path: &Path, now: SystemTime, min_age_secs: u64) -> Result<bool, VaultError> {
    if min_age_secs == 0 {
        return Ok(true);
    }
    let modified = fs::metadata(path)
        .map_err(VaultError::Io)?
        .modified()
        .map_err(|err| VaultError::InvalidLayout {
            detail: err.to_string(),
        })?;
    let age = now
        .duration_since(modified)
        .unwrap_or_default()
        .as_secs();
    Ok(age >= min_age_secs)
}

fn list_managed_blob_files(vault_root: &Path) -> Result<Vec<(String, PathBuf)>, VaultError> {
    let blobs_root = vault_root.join(BLOBS_DIR);
    if !blobs_root.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for shard_a in fs::read_dir(&blobs_root).map_err(VaultError::Io)? {
        let shard_a = shard_a.map_err(VaultError::Io)?;
        if !shard_a.file_type().map_err(VaultError::Io)?.is_dir() {
            continue;
        }
        for shard_b in fs::read_dir(shard_a.path()).map_err(VaultError::Io)? {
            let shard_b = shard_b.map_err(VaultError::Io)?;
            if !shard_b.file_type().map_err(VaultError::Io)?.is_dir() {
                continue;
            }
            for entry in fs::read_dir(shard_b.path()).map_err(VaultError::Io)? {
                let entry = entry.map_err(VaultError::Io)?;
                if !entry.file_type().map_err(VaultError::Io)?.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(hash) = hash_from_blob_filename(&name) {
                    files.push((hash, entry.path()));
                }
            }
        }
    }
    Ok(files)
}

fn hash_from_blob_filename(name: &str) -> Option<String> {
    let (stem, _) = name.split_once('.')?;
    if stem.len() == 64 && stem.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(stem.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cas::store_managed;
    use crate::import::db::insert_entity;
    use crate::layout::blob_path;
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn sweep_dry_run_lists_orphan_without_deleting() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let hash = "d".repeat(64);
        let path = blob_path(vault.root(), &hash, ".jpg").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"orphan").unwrap();

        let report = sweep_unreferenced_blobs(
            vault.connection(),
            vault.root(),
            BlobGcOptions {
                dry_run: true,
                min_age_secs: 0,
            },
        )
        .unwrap();

        assert_eq!(report.candidates, 1);
        assert_eq!(report.removed, 0);
        assert!(path.exists());
    }

    #[test]
    fn sweep_removes_orphan_when_unreferenced() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let hash = "e".repeat(64);
        let path = blob_path(vault.root(), &hash, ".jpg").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"orphan").unwrap();

        let report = sweep_unreferenced_blobs(
            vault.connection(),
            vault.root(),
            BlobGcOptions {
                dry_run: false,
                min_age_secs: 0,
            },
        )
        .unwrap();

        assert_eq!(report.removed, 1);
        assert!(!path.exists());
    }

    #[test]
    fn sweep_skips_pinned_blob() {
        use chrono::Utc;
        use uuid::Uuid;

        use crate::import::db::insert_entity;

        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let staging = dir.path().join("staging/job/files/thumb.jpg");
        fs::create_dir_all(staging.parent().unwrap()).unwrap();
        fs::write(&staging, b"pinned").unwrap();
        let stored = store_managed(vault.root(), &staging, ".jpg").unwrap();
        let now = Utc::now().to_rfc3339();
        let entity_id = Uuid::new_v4();
        insert_entity(
            vault.connection(),
            entity_id,
            Some(&stored.content_hash),
            Some("image/jpeg"),
            stored.size,
            "active",
            &now,
            None,
        )
        .unwrap();

        let report = sweep_unreferenced_blobs(
            vault.connection(),
            vault.root(),
            BlobGcOptions {
                dry_run: false,
                min_age_secs: 0,
            },
        )
        .unwrap();

        assert_eq!(report.removed, 0);
        assert!(stored.blob_path.exists());
    }

    #[test]
    fn sweep_respects_min_age() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let hash = "f".repeat(64);
        let path = blob_path(vault.root(), &hash, ".jpg").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"fresh").unwrap();

        let report = sweep_unreferenced_blobs(
            vault.connection(),
            vault.root(),
            BlobGcOptions {
                dry_run: false,
                min_age_secs: 86_400,
            },
        )
        .unwrap();

        assert_eq!(report.candidates, 0);
        assert!(path.exists());
    }

    #[test]
    fn remove_blob_if_unreferenced_keeps_shared_hash() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let hash = "1".repeat(64);
        let path = blob_path(vault.root(), &hash, ".jpg").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"shared").unwrap();
        let now = Utc::now().to_rfc3339();
        let entity_a = Uuid::new_v4();
        let entity_b = Uuid::new_v4();
        insert_entity(vault.connection(), entity_a, Some(&hash), Some("image/jpeg"), 1, "active", &now, None)
            .unwrap();
        insert_entity(vault.connection(), entity_b, Some(&hash), Some("image/jpeg"), 1, "active", &now, None)
            .unwrap();

        let removed = remove_blob_if_unreferenced(
            vault.connection(),
            vault.root(),
            &hash,
            Some(".jpg"),
            Some("image/jpeg"),
        )
        .unwrap();
        assert!(!removed);
        assert!(path.exists());

        vault
            .connection()
            .execute(
                "UPDATE entity SET status = 'user_deleted' WHERE id = ?1",
                [entity_b.to_string()],
            )
            .unwrap();
        let removed = remove_blob_if_unreferenced(
            vault.connection(),
            vault.root(),
            &hash,
            Some(".jpg"),
            Some("image/jpeg"),
        )
        .unwrap();
        assert!(!removed);
        assert!(path.exists());
    }
}
