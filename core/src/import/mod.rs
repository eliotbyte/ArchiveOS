pub mod db;

use std::fs;
use std::path::{Path, PathBuf};

use archiveos_contract::{
    ImportManifest, ImportReport, ImportStrategy, ManifestItem, ManifestSourceRef, VaultError,
};
use chrono::Utc;
use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::cas::{self, CasStoreResult};
use crate::Vault;

pub fn import(
    vault: &Vault,
    staging_dir: &Path,
    manifest: Option<&ImportManifest>,
    strategy: ImportStrategy,
) -> Result<ImportReport, VaultError> {
    let mut report = ImportReport::default();
    let conn = vault.connection();

    if let Some(manifest) = manifest {
        import_from_manifest(vault, conn, staging_dir, manifest, strategy, &mut report)?;
        if strategy == ImportStrategy::Managed {
            cleanup_staging(staging_dir)?;
        }
    } else {
        import_scan(conn, staging_dir, &mut report)?;
    }

    Ok(report)
}

fn import_from_manifest(
    vault: &Vault,
    conn: &Connection,
    staging_dir: &Path,
    manifest: &ImportManifest,
    strategy: ImportStrategy,
    report: &mut ImportReport,
) -> Result<(), VaultError> {
    if let Some(collection) = &manifest.collection {
        report.collection_id = Some(import_collection(
            conn,
            collection,
            &manifest.source,
        )?);
    }

    for item in &manifest.items {
        if item.status != "complete" {
            report.items_skipped += 1;
            continue;
        }
        import_manifest_item(vault, conn, staging_dir, item, strategy, report)?;
    }

    if let Some(collection_id) = report.collection_id {
        let default_source = manifest
            .items
            .first()
            .and_then(|i| i.source_ref.as_ref())
            .map(|r| r.source.as_str())
            .unwrap_or(manifest.source.as_str());

        for member in &manifest.membership {
            if db::link_collection_member(
                conn,
                collection_id,
                default_source,
                "video",
                &member.external_id,
                member.position,
            )? {
                // linked
            }
        }
    }

    Ok(())
}

fn import_collection(
    conn: &Connection,
    collection: &archiveos_contract::ManifestCollection,
    manifest_source: &str,
) -> Result<Uuid, VaultError> {
    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();

    db::insert_entity(
        conn,
        id,
        None,
        None,
        0,
        "present",
        &now,
        None,
    )?;

    db::insert_collection(conn, id, &collection.collection_type, &collection.title)?;

    db::insert_source_ref(
        conn,
        Uuid::new_v4(),
        id,
        manifest_source,
        "playlist",
        &collection.external_id,
        Some(&collection.url),
        "live",
    )?;

    Ok(id)
}

fn import_manifest_item(
    vault: &Vault,
    conn: &Connection,
    staging_dir: &Path,
    item: &ManifestItem,
    strategy: ImportStrategy,
    report: &mut ImportReport,
) -> Result<(), VaultError> {
    let file_path = resolve_item_path(staging_dir, &item.path, strategy)?;
    let ext = extension_from_path(&file_path);

    let (content_hash, size, mime, blob_stored) = match strategy {
        ImportStrategy::Managed => {
            let cas = cas::store_managed(vault.root(), &file_path, &ext)?;
            let result = (
                cas.content_hash.clone(),
                cas.size,
                guess_mime(&file_path),
                true,
            );
            track_blob(&cas, report);
            result
        }
        ImportStrategy::Reference => {
            let hash = cas::hash_file(&file_path)?;
            if let Some(expected) = &item.sha256
                && !expected.eq_ignore_ascii_case(&hash)
            {
                return Err(VaultError::InvalidLayout {
                    detail: format!(
                        "sha256 mismatch for {}: expected {expected}, got {hash}",
                        file_path.display()
                    ),
                });
            }
            let size = fs::metadata(&file_path)?.len();
            (hash, size, guess_mime(&file_path), false)
        }
    };

    let entity_id = resolve_or_create_entity(
        conn,
        item.source_ref.as_ref(),
        &content_hash,
        &mime,
        size,
        report,
    )?;

    db::update_entity_content(conn, entity_id, &content_hash, &mime, size)?;

    if strategy == ImportStrategy::Reference {
        let path_str = file_path
            .canonicalize()
            .unwrap_or(file_path)
            .to_string_lossy()
            .into_owned();
        db::upsert_metadata(conn, entity_id, "path", &path_str, "system")?;
    }

    if let Some(source_ref) = &item.source_ref {
        db::upsert_source_ref(conn, entity_id, source_ref)?;
    }

    if let Some(metadata) = &item.metadata {
        ingest_metadata(conn, entity_id, metadata)?;
    }

    let _ = blob_stored;
    Ok(())
}

fn import_scan(
    conn: &Connection,
    scan_dir: &Path,
    report: &mut ImportReport,
) -> Result<(), VaultError> {
    if !scan_dir.is_dir() {
        return Err(VaultError::InvalidLayout {
            detail: format!("scan directory not found: {}", scan_dir.display()),
        });
    }

    for entry in WalkDir::new(scan_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let file_path = entry.into_path();
        let content_hash = cas::hash_file(&file_path)?;
        let size = fs::metadata(&file_path)?.len();
        let mime = guess_mime(&file_path);

        let entity_id = resolve_or_create_entity(conn, None, &content_hash, &mime, size, report)?;

        db::update_entity_content(conn, entity_id, &content_hash, &mime, size)?;

        let path_str = file_path
            .canonicalize()
            .unwrap_or(file_path.clone())
            .to_string_lossy()
            .into_owned();
        db::upsert_metadata(conn, entity_id, "path", &path_str, "system")?;

        if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
            db::upsert_metadata(conn, entity_id, "title", name, "inferred")?;
        }
    }

    Ok(())
}

fn resolve_or_create_entity(
    conn: &Connection,
    source_ref: Option<&ManifestSourceRef>,
    content_hash: &str,
    mime: &str,
    size: u64,
    report: &mut ImportReport,
) -> Result<Uuid, VaultError> {
    if let Some(source_ref) = source_ref
        && let Some(existing) = db::find_entity_by_source_ref(
            conn,
            &source_ref.source,
            &source_ref.kind,
            &source_ref.external_id,
        )?
    {
        report.entities_reused += 1;
        return Ok(existing);
    }

    if let Some(existing) = db::find_entity_by_content_hash(conn, content_hash)? {
        report.entities_reused += 1;
        return Ok(existing);
    }

    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    db::insert_entity(conn, id, Some(content_hash), Some(mime), size, "present", &now, None)?;
    report.entities_created += 1;
    Ok(id)
}

fn resolve_item_path(
    staging_dir: &Path,
    item_path: &str,
    strategy: ImportStrategy,
) -> Result<PathBuf, VaultError> {
    let path = PathBuf::from(item_path);
    let resolved = if path.is_absolute() {
        path
    } else {
        staging_dir.join(path)
    };

    if !resolved.is_file() {
        return Err(VaultError::InvalidLayout {
            detail: format!(
                "import file not found ({}): {}",
                match strategy {
                    ImportStrategy::Managed => "managed",
                    ImportStrategy::Reference => "reference",
                },
                resolved.display()
            ),
        });
    }

    Ok(resolved)
}

fn ingest_metadata(conn: &Connection, entity_id: Uuid, metadata: &Value) -> Result<(), VaultError> {
    let Some(map) = metadata.as_object() else {
        return Ok(());
    };

    for (key, value) in map {
        db::upsert_metadata(conn, entity_id, key, &value_to_string(value), "source")?;
    }
    Ok(())
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn guess_mime(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
}

fn extension_from_path(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default()
}

fn track_blob(cas: &CasStoreResult, report: &mut ImportReport) {
    if cas.deduped {
        report.blobs_deduped += 1;
    } else {
        report.blobs_stored += 1;
    }
}

fn cleanup_staging(staging_dir: &Path) -> Result<(), VaultError> {
    if staging_dir.exists() {
        fs::remove_dir_all(staging_dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use archiveos_contract::{ManifestCollection, ManifestSourceRef};
    use tempfile::tempdir;

    fn sample_manifest(staging_rel: &str) -> ImportManifest {
        ImportManifest {
            source: "test".into(),
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            items: vec![ManifestItem {
                path: staging_rel.into(),
                sha256: None,
                status: "complete".into(),
                source_ref: Some(ManifestSourceRef {
                    source: "youtube".into(),
                    kind: "video".into(),
                    external_id: "vid123".into(),
                    url: Some("https://example.com".into()),
                }),
                metadata: Some(serde_json::json!({
                    "title": "Test Video",
                    "duration": 120
                })),
            }],
            membership: vec![],
        }
    }

    #[test]
    fn import_managed_stores_blob_and_creates_entity() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        let file = staging.join("files/video.mp4");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"video-content").unwrap();

        let manifest = sample_manifest("files/video.mp4");
        let report = import(
            &vault,
            &staging,
            Some(&manifest),
            ImportStrategy::Managed,
        )
        .unwrap();

        assert_eq!(report.entities_created, 1);
        assert_eq!(report.blobs_stored, 1);
        assert!(!file.exists());

        let count: i32 = vault
            .connection()
            .query_row("SELECT COUNT(*) FROM entity", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let title: String = vault
            .connection()
            .query_row(
                "SELECT value FROM metadata WHERE key = 'title' AND provenance = 'source'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(title, "Test Video");
    }

    #[test]
    fn import_managed_dedup_reuses_entity_by_source_ref() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging1 = dir.path().join("staging/job-1");
        let file1 = staging1.join("files/a.mp4");
        fs::create_dir_all(file1.parent().unwrap()).unwrap();
        fs::write(&file1, b"same").unwrap();

        let manifest1 = sample_manifest("files/a.mp4");
        import(
            &vault,
            &staging1,
            Some(&manifest1),
            ImportStrategy::Managed,
        )
        .unwrap();

        let staging2 = dir.path().join("staging/job-2");
        let file2 = staging2.join("files/b.mp4");
        fs::create_dir_all(file2.parent().unwrap()).unwrap();
        fs::write(&file2, b"same").unwrap();

        let report = import(
            &vault,
            &staging2,
            Some(&manifest2_from("files/b.mp4")),
            ImportStrategy::Managed,
        )
        .unwrap();

        assert_eq!(report.entities_reused, 1);
        assert_eq!(report.blobs_deduped, 1);

        let count: i32 = vault
            .connection()
            .query_row("SELECT COUNT(*) FROM entity", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    fn manifest2_from(path: &str) -> ImportManifest {
        sample_manifest(path)
    }

    #[test]
    fn import_reference_does_not_move_file() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let user_file = dir.path().join("photos/pic.jpg");
        fs::create_dir_all(user_file.parent().unwrap()).unwrap();
        fs::write(&user_file, b"jpeg-bytes").unwrap();

        let manifest = ImportManifest {
            source: "scan".into(),
            vault: "test".into(),
            strategy: ImportStrategy::Reference,
            collection: None,
            items: vec![ManifestItem {
                path: user_file.to_string_lossy().into_owned(),
                sha256: None,
                status: "complete".into(),
                source_ref: None,
                metadata: None,
            }],
            membership: vec![],
        };

        import(
            &vault,
            &dir.path().join("staging/unused"),
            Some(&manifest),
            ImportStrategy::Reference,
        )
        .unwrap();

        assert!(user_file.exists());
        let path_meta: String = vault
            .connection()
            .query_row(
                "SELECT value FROM metadata WHERE key = 'path'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(path_meta.ends_with("pic.jpg"));
    }

    #[test]
    fn import_scan_without_manifest_indexes_directory() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let scan_root = dir.path().join("my-pics");
        fs::create_dir_all(scan_root.join("nested")).unwrap();
        fs::write(scan_root.join("nested/a.png"), b"png-a").unwrap();
        fs::write(scan_root.join("b.png"), b"png-b").unwrap();

        let report = import(&vault, &scan_root, None, ImportStrategy::Reference).unwrap();

        assert_eq!(report.entities_created, 2);

        let count: i32 = vault
            .connection()
            .query_row("SELECT COUNT(*) FROM entity", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn import_collection_creates_membership_edges() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();

        let staging0 = dir.path().join("staging/job-0");
        let file0 = staging0.join("files/v0.mp4");
        fs::create_dir_all(file0.parent().unwrap()).unwrap();
        fs::write(&file0, b"v0").unwrap();
        let mut manifest0 = sample_manifest("files/v0.mp4");
        manifest0.items[0].source_ref.as_mut().unwrap().external_id = "ext-0".into();
        import(
            &vault,
            &staging0,
            Some(&manifest0),
            ImportStrategy::Managed,
        )
        .unwrap();

        let staging1 = dir.path().join("staging/job-1");
        let file1 = staging1.join("files/v1.mp4");
        fs::create_dir_all(file1.parent().unwrap()).unwrap();
        fs::write(&file1, b"v1").unwrap();

        let mut manifest = sample_manifest("files/v1.mp4");
        manifest.items[0].source_ref.as_mut().unwrap().external_id = "ext-1".into();
        manifest.collection = Some(ManifestCollection {
            collection_type: "youtube_playlist".into(),
            external_id: "PL123".into(),
            url: "https://youtube.com/playlist?list=PL123".into(),
            title: "My List".into(),
        });
        manifest.membership = vec![
            archiveos_contract::ManifestMembership {
                external_id: "ext-0".into(),
                position: 0,
            },
            archiveos_contract::ManifestMembership {
                external_id: "ext-1".into(),
                position: 1,
            },
        ];

        let report = import(
            &vault,
            &staging1,
            Some(&manifest),
            ImportStrategy::Managed,
        )
        .unwrap();

        assert!(report.collection_id.is_some());

        let members: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM collection_member WHERE collection_id = ?1",
                [report.collection_id.unwrap().to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(members, 2);
    }
}
