pub mod db;

use std::fs;
use std::path::{Path, PathBuf};

use archiveos_contract::{
    ImportManifest, ImportReport, ImportStrategy, ManifestAssetCatalogEntry, ManifestChannel,
    ManifestChannelAvatar, ManifestItem, ManifestRelation, ManifestSourceRef, VaultError,
};
use chrono::Utc;
use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::assets::UpsertAssetInput;
use crate::cas::{self, CasStoreResult};
use crate::Vault;

pub struct ImportOptions {
    pub cleanup_staging: bool,
    pub finalize_membership: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            cleanup_staging: true,
            finalize_membership: true,
        }
    }
}

pub fn import(
    vault: &Vault,
    staging_dir: &Path,
    manifest: Option<&ImportManifest>,
    strategy: ImportStrategy,
) -> Result<ImportReport, VaultError> {
    import_with_options(vault, staging_dir, manifest, strategy, ImportOptions::default())
}

pub fn import_with_options(
    vault: &Vault,
    staging_dir: &Path,
    manifest: Option<&ImportManifest>,
    strategy: ImportStrategy,
    options: ImportOptions,
) -> Result<ImportReport, VaultError> {
    let mut report = ImportReport::default();
    let conn = vault.connection();

    if let Some(manifest) = manifest {
        import_from_manifest(
            vault,
            conn,
            staging_dir,
            manifest,
            strategy,
            options.finalize_membership,
            &mut report,
        )?;
        if strategy == ImportStrategy::Managed && options.cleanup_staging {
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
    finalize_membership: bool,
    report: &mut ImportReport,
) -> Result<(), VaultError> {
    for channel in &manifest.channels {
        import_channel(vault, conn, staging_dir, strategy, channel)?;
    }

    if let Some(collection) = &manifest.collection {
        report.collection_id = Some(import_collection(
            conn,
            collection,
            manifest,
        )?);
    }

    for item in &manifest.items {
        import_manifest_item(vault, conn, staging_dir, item, strategy, &manifest.source, report)?;
    }

    if let Some(collection_id) = report.collection_id {
        let member_source = manifest_source_identity(manifest)
            .ok_or_else(|| VaultError::InvalidLayout {
                detail: "manifest missing source_identity for collection membership".into(),
            })?;
        let mut active_members = Vec::new();

        for member in &manifest.membership {
            let entity_id = db::resolve_or_create_entity_by_source_ref(
                conn,
                &member_source,
                &member.kind,
                &member.external_id,
                member.url.as_deref(),
            )?;
            if let Some(linked_id) = db::link_collection_member(
                conn,
                collection_id,
                &member_source,
                &member.kind,
                &member.external_id,
                member.position,
            )? {
                active_members.push(linked_id);
            } else {
                active_members.push(entity_id);
            }
        }
        if finalize_membership {
            db::mark_collection_members_removed_except(conn, collection_id, &active_members)?;
        }
    }

    for relation in &manifest.relations {
        import_relation(conn, relation)?;
    }

    ensure_uploaded_by_relations_from_manifest(conn, manifest)?;

    Ok(())
}

fn channel_external_id_from_item(item: &ManifestItem) -> Option<String> {
    if let Some(ytdlp) = item.metadata_by_provenance.get("yt-dlp") {
        if let Some(channel_id) = ytdlp.get("channel_id").and_then(|v| v.as_str()) {
            if !channel_id.is_empty() {
                return Some(channel_id.to_string());
            }
        }
    }
    if let Some(metadata) = &item.metadata {
        if let Some(channel_id) = metadata.get("channel_id").and_then(|v| v.as_str()) {
            if !channel_id.is_empty() {
                return Some(channel_id.to_string());
            }
        }
    }
    None
}

fn ensure_uploaded_by_relations_from_manifest(
    conn: &Connection,
    manifest: &ImportManifest,
) -> Result<(), VaultError> {
    let platform = manifest_source_identity(manifest).unwrap_or_else(|| "youtube".into());
    let platform = db::canonical_platform_source(&platform);

    for item in &manifest.items {
        if item.status != "complete" {
            continue;
        }
        let Some(video_ref) = item.source_ref.as_ref() else {
            continue;
        };
        if video_ref.kind != "video" {
            continue;
        }
        let Some(video_entity_id) = db::find_entity_by_source_ref(
            conn,
            platform,
            &video_ref.kind,
            &video_ref.external_id,
        )?
        else {
            continue;
        };
        if crate::channels::resolve_uploaded_by_relation(conn, video_entity_id)?.is_some() {
            continue;
        }
        let Some(channel_external_id) = channel_external_id_from_item(item) else {
            continue;
        };
        let Some(channel_entity_id) =
            db::find_entity_by_source_ref(conn, platform, "channel", &channel_external_id)?
        else {
            continue;
        };
        db::link_entity_relation(conn, video_entity_id, channel_entity_id, "uploaded_by")?;
    }

    Ok(())
}

fn import_channel(
    vault: &Vault,
    conn: &Connection,
    staging_dir: &Path,
    strategy: ImportStrategy,
    channel: &ManifestChannel,
) -> Result<Uuid, VaultError> {
    let entity_id = if let Some(existing) =
        db::find_entity_by_source_ref(conn, &channel.source, &channel.kind, &channel.external_id)?
    {
        existing
    } else {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        db::insert_entity(conn, id, None, None, 0, "active", &now, None)?;
        db::insert_source_ref(
            conn,
            Uuid::new_v4(),
            id,
            &channel.source,
            &channel.kind,
            &channel.external_id,
            channel.url.as_deref(),
            "live",
        )?;
        id
    };

    if let Some(metadata) = &channel.metadata {
        ingest_metadata_legacy(conn, entity_id, metadata)?;
    }
    ingest_metadata_by_provenance(conn, entity_id, &channel.metadata_by_provenance)?;

    if let Some(avatar) = &channel.avatar {
        import_channel_avatar(vault, conn, staging_dir, strategy, entity_id, avatar)?;
    }

    Ok(entity_id)
}

fn import_channel_avatar(
    vault: &Vault,
    conn: &Connection,
    staging_dir: &Path,
    strategy: ImportStrategy,
    channel_entity_id: Uuid,
    avatar: &ManifestChannelAvatar,
) -> Result<(), VaultError> {
    let file_path = resolve_item_path(staging_dir, &avatar.path, strategy)?;
    let ext = extension_from_path(&file_path);

    let (content_hash, size, mime, asset_path) = match strategy {
        ImportStrategy::Managed => {
            let cas = cas::store_managed(vault.root(), &file_path, &ext)?;
            (
                cas.content_hash,
                cas.size,
                guess_mime(&cas.blob_path),
                Some(cas.blob_path.to_string_lossy().into_owned()),
            )
        }
        ImportStrategy::Reference => {
            let hash = cas::hash_file(&file_path)?;
            let size = fs::metadata(&file_path)?.len();
            let path_str = file_path
                .canonicalize()
                .unwrap_or(file_path.clone())
                .to_string_lossy()
                .into_owned();
            (hash, size, guess_mime(&file_path), Some(path_str))
        }
    };

    let asset_id = crate::assets::upsert_preview_asset(
        conn,
        channel_entity_id,
        "avatar",
        "avatar",
        &UpsertAssetInput {
            entity_id: channel_entity_id,
            role: "supporting",
            kind: "avatar",
            content_hash: Some(&content_hash),
            mime: Some(&mime),
            size,
            ext: Some(&ext),
            status: "present",
            storage_strategy: match strategy {
                ImportStrategy::Managed => "managed",
                ImportStrategy::Reference => "reference",
            },
            path: asset_path.as_deref(),
        },
    )?;
    crate::assets::upsert_asset_metadata(conn, asset_id, "preview_role", "avatar", "archiveos")?;
    if let Some(source_url) = &avatar.source_url {
        crate::assets::upsert_asset_metadata(conn, asset_id, "source_url", source_url, "archiveos")?;
    }
    Ok(())
}

fn import_relation(conn: &Connection, relation: &ManifestRelation) -> Result<(), VaultError> {
    let source = db::canonical_platform_source(&relation.source);
    let Some(from_id) = db::find_entity_by_source_ref(
        conn,
        source,
        &relation.from_kind,
        &relation.from_external_id,
    )?
    else {
        return Ok(());
    };
    let Some(to_id) = db::find_entity_by_source_ref(
        conn,
        source,
        &relation.to_kind,
        &relation.to_external_id,
    )?
    else {
        return Ok(());
    };

    db::link_entity_relation(conn, from_id, to_id, &relation.relation)?;

    if relation.relation == "thumbnail" {
        db::upsert_metadata(
            conn,
            from_id,
            "thumbnail_external_id",
            &relation.to_external_id,
            "archiveos",
        )?;
    }

    Ok(())
}

fn collection_source_kind(collection_type: &str) -> &'static str {
    if collection_type.contains("channel") {
        "channel"
    } else {
        "playlist"
    }
}

fn import_collection(
    conn: &Connection,
    collection: &archiveos_contract::ManifestCollection,
    manifest: &ImportManifest,
) -> Result<Uuid, VaultError> {
    let source = manifest_source_identity(manifest).ok_or_else(|| VaultError::InvalidLayout {
        detail: "manifest missing source_identity for collection import".into(),
    })?;
    let source_kind = collection_source_kind(&collection.collection_type);

    if source_kind == "playlist" {
        let playlist_ids =
            db::find_playlist_entity_ids_by_external_id(conn, &collection.external_id)?;
        if let Some(canonical) = playlist_ids.first().copied() {
            for duplicate in playlist_ids.iter().skip(1) {
                db::merge_collection_members(conn, *duplicate, canonical)?;
            }
            db::upsert_source_ref(
                conn,
                canonical,
                &archiveos_contract::ManifestSourceRef {
                    source: db::canonical_platform_source(&source).to_string(),
                    kind: "playlist".into(),
                    external_id: collection.external_id.clone(),
                    url: Some(collection.url.clone()),
                },
            )?;
            return Ok(canonical);
        }
    }

    if let Some(existing) = db::find_entity_by_source_ref(
        conn,
        &source,
        source_kind,
        &collection.external_id,
    )? {
        return Ok(existing);
    }

    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();

    db::insert_entity(
        conn,
        id,
        None,
        None,
        0,
        "active",
        &now,
        None,
    )?;

    db::insert_collection(conn, id, &collection.collection_type, &collection.title)?;

    db::insert_source_ref(
        conn,
        Uuid::new_v4(),
        id,
        &source,
        source_kind,
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
    manifest_source: &str,
    report: &mut ImportReport,
) -> Result<(), VaultError> {
    if item.status != "complete" {
        import_logical_manifest_item(conn, item, manifest_source)?;
        report.items_skipped += 1;
        return Ok(());
    }

    let file_path = resolve_item_path(staging_dir, &item.path, strategy)?;
    let ext = extension_from_path(&file_path);
    let image_meta = crate::media::image::extract(&file_path);
    let modified_at = file_modified_at(&file_path);

    let (content_hash, size, mime, asset_path, blob_stored) = match strategy {
        ImportStrategy::Managed => {
            let cas = cas::store_managed(vault.root(), &file_path, &ext)?;
            let asset_path = cas.blob_path.to_string_lossy().into_owned();
            let result = (
                cas.content_hash.clone(),
                cas.size,
                guess_mime(&cas.blob_path),
                Some(asset_path),
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
            let path_str = file_path
                .canonicalize()
                .unwrap_or(file_path.clone())
                .to_string_lossy()
                .into_owned();
            (hash, size, guess_mime(&file_path), Some(path_str), false)
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

    let (asset_role, asset_kind) = asset_role_and_kind(item.source_ref.as_ref());
    let asset_id = crate::assets::upsert_asset(
        conn,
        &UpsertAssetInput {
            entity_id,
            role: asset_role,
            kind: asset_kind,
            content_hash: Some(&content_hash),
            mime: Some(&mime),
            size,
            ext: Some(&ext),
            status: "present",
            storage_strategy: match strategy {
                ImportStrategy::Managed => "managed",
                ImportStrategy::Reference => "reference",
            },
            path: asset_path.as_deref(),
        },
    )?;
    if asset_kind == "thumbnail" {
        crate::assets::supersede_present_assets(conn, entity_id, "thumbnail", asset_id)?;
    }

    if strategy == ImportStrategy::Reference {
        if let Some(path_str) = &asset_path {
            db::upsert_metadata(conn, entity_id, "path", path_str, "system")?;
        }
    }

    if let Some(source_ref) = &item.source_ref {
        db::upsert_source_ref(conn, entity_id, source_ref)?;
    }

    if let Some(metadata) = &item.metadata {
        let default_provenance = default_metadata_provenance(manifest_source);
        ingest_metadata(conn, entity_id, metadata, default_provenance)?;
    }

    ingest_metadata_by_provenance(conn, entity_id, &item.metadata_by_provenance)?;
    import_catalog_assets(conn, entity_id, &item.assets)?;

    crate::media::image::persist(conn, entity_id, &image_meta, modified_at.as_deref())?;

    let _ = blob_stored;
    Ok(())
}

fn import_logical_manifest_item(
    conn: &Connection,
    item: &ManifestItem,
    manifest_source: &str,
) -> Result<(), VaultError> {
    let Some(source_ref) = &item.source_ref else {
        return Ok(());
    };
    let entity_id = db::resolve_or_create_entity_by_source_ref(
        conn,
        &source_ref.source,
        &source_ref.kind,
        &source_ref.external_id,
        source_ref.url.as_deref(),
    )?;
    if let Some(status) = source_status_from_item_status(&item.status) {
        db::update_source_ref_status(
            conn,
            &source_ref.source,
            &source_ref.kind,
            &source_ref.external_id,
            status,
        )?;
    }
    if let Some(metadata) = &item.metadata {
        let default_provenance = default_metadata_provenance(manifest_source);
        ingest_metadata(conn, entity_id, metadata, default_provenance)?;
    }
    ingest_metadata_by_provenance(conn, entity_id, &item.metadata_by_provenance)?;
    import_catalog_assets(conn, entity_id, &item.assets)?;
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
        let image_meta = crate::media::image::extract(&file_path);
        let modified_at = file_modified_at(&file_path);
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
        let ext = extension_from_path(&file_path);
        crate::assets::upsert_asset(
            conn,
            &UpsertAssetInput {
                entity_id,
                role: "primary",
                kind: "file",
                content_hash: Some(&content_hash),
                mime: Some(&mime),
                size,
                ext: Some(&ext),
                status: "present",
                storage_strategy: "reference",
                path: Some(&path_str),
            },
        )?;
        db::upsert_metadata(conn, entity_id, "path", &path_str, "system")?;

        if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
            db::upsert_metadata(conn, entity_id, "title", name, "inferred")?;
        }

        crate::media::image::persist(conn, entity_id, &image_meta, modified_at.as_deref())?;
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
    db::insert_entity(conn, id, Some(content_hash), Some(mime), size, "active", &now, None)?;
    report.entities_created += 1;
    Ok(id)
}

fn manifest_source_identity(manifest: &ImportManifest) -> Option<String> {
    if let Some(source_identity) = &manifest.source_identity {
        return Some(source_identity.clone());
    }
    for item in &manifest.items {
        if let Some(source_ref) = &item.source_ref {
            return Some(source_ref.source.clone());
        }
    }
    // Legacy yt-dlp manifests without explicit source_identity.
    if manifest.source == "yt-dlp" {
        return Some("youtube".into());
    }
    None
}

fn import_catalog_assets(
    conn: &Connection,
    entity_id: Uuid,
    assets: &[ManifestAssetCatalogEntry],
) -> Result<(), VaultError> {
    for entry in assets {
        crate::assets::upsert_catalog_asset(conn, entity_id, entry)?;
    }
    Ok(())
}

fn asset_role_and_kind(source_ref: Option<&ManifestSourceRef>) -> (&'static str, &'static str) {
    match source_ref.map(|source_ref| source_ref.kind.as_str()) {
        Some("thumbnail") => ("supporting", "thumbnail"),
        Some("subtitle") => ("supporting", "subtitle"),
        Some("audio") => ("supporting", "audio"),
        Some("video") => ("primary", "video"),
        _ => ("primary", "file"),
    }
}

fn source_status_from_item_status(status: &str) -> Option<&'static str> {
    match status {
        "dead" => Some("dead"),
        "private" => Some("private"),
        "region_locked" => Some("region_locked"),
        "unavailable" => Some("unavailable"),
        _ => None,
    }
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

fn default_metadata_provenance(manifest_source: &str) -> &'static str {
    if manifest_source == "yt-dlp" {
        "yt-dlp"
    } else {
        "source"
    }
}

fn ingest_metadata(
    conn: &Connection,
    entity_id: Uuid,
    metadata: &Value,
    provenance: &str,
) -> Result<(), VaultError> {
    ingest_metadata_map(conn, entity_id, metadata, provenance)
}

fn ingest_metadata_by_provenance(
    conn: &Connection,
    entity_id: Uuid,
    groups: &std::collections::BTreeMap<String, Value>,
) -> Result<(), VaultError> {
    for (provenance, metadata) in groups {
        ingest_metadata_map(conn, entity_id, metadata, provenance)?;
    }
    Ok(())
}

fn ingest_metadata_map(
    conn: &Connection,
    entity_id: Uuid,
    metadata: &Value,
    provenance: &str,
) -> Result<(), VaultError> {
    let Some(map) = metadata.as_object() else {
        return Ok(());
    };

    for (key, value) in map {
        if !should_ingest_metadata_value(key, value) {
            continue;
        }
        db::upsert_metadata(conn, entity_id, key, &value_to_string(value), provenance)?;
    }
    Ok(())
}

fn should_ingest_metadata_value(key: &str, value: &Value) -> bool {
    if value.is_null() {
        return false;
    }
    if matches!(value, Value::String(s) if s.is_empty())
        && matches!(key, "title" | "description")
    {
        return false;
    }
    true
}

fn ingest_metadata_legacy(conn: &Connection, entity_id: Uuid, metadata: &Value) -> Result<(), VaultError> {
    ingest_metadata(conn, entity_id, metadata, "source")
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

fn file_modified_at(path: &Path) -> Option<String> {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .map(|time| chrono::DateTime::<Utc>::from(time).to_rfc3339())
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
    use rusqlite::params;
    use tempfile::tempdir;

    fn sample_manifest(staging_rel: &str) -> ImportManifest {
        ImportManifest {
            source: "test".into(),
            source_identity: Some("youtube".into()),
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
                metadata_by_provenance: Default::default(),
                assets: vec![],
            }],
            membership: vec![],
            channels: vec![],
            relations: vec![],
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
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Reference,
            collection: None,
            items: vec![ManifestItem {
                path: user_file.to_string_lossy().into_owned(),
                sha256: None,
                status: "complete".into(),
                source_ref: None,
                metadata: None,
                metadata_by_provenance: Default::default(),
                assets: vec![],
            }],
            membership: vec![],
            channels: vec![],
            relations: vec![],
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
                kind: "video".into(),
                url: None,
            },
            archiveos_contract::ManifestMembership {
                external_id: "ext-1".into(),
                position: 1,
                kind: "video".into(),
                url: None,
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

    #[test]
    fn import_collection_reuses_existing_playlist_source_ref() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();

        let staging0 = dir.path().join("staging/job-0");
        let file0 = staging0.join("files/v0.mp4");
        fs::create_dir_all(file0.parent().unwrap()).unwrap();
        fs::write(&file0, b"v0").unwrap();
        let mut manifest0 = sample_manifest("files/v0.mp4");
        manifest0.items[0].source_ref.as_mut().unwrap().external_id = "ext-0".into();
        manifest0.collection = Some(ManifestCollection {
            collection_type: "youtube_playlist".into(),
            external_id: "PL123".into(),
            url: "https://youtube.com/playlist?list=PL123".into(),
            title: "My List".into(),
        });
        manifest0.membership = vec![archiveos_contract::ManifestMembership {
            external_id: "ext-0".into(),
            position: 0,
            kind: "video".into(),
            url: None,
        }];
        let report0 = import(
            &vault,
            &staging0,
            Some(&manifest0),
            ImportStrategy::Managed,
        )
        .unwrap();
        let collection_id = report0.collection_id.unwrap();

        let staging1 = dir.path().join("staging/job-1");
        fs::create_dir_all(&staging1).unwrap();
        let mut manifest1 = sample_manifest("files/missing.mp4");
        manifest1.items.clear();
        manifest1.collection = Some(ManifestCollection {
            collection_type: "youtube_playlist".into(),
            external_id: "PL123".into(),
            url: "https://youtube.com/playlist?list=PL123".into(),
            title: "My List".into(),
        });
        manifest1.membership = vec![archiveos_contract::ManifestMembership {
            external_id: "ext-0".into(),
            position: 0,
            kind: "video".into(),
            url: None,
        }];

        let report1 = import(
            &vault,
            &staging1,
            Some(&manifest1),
            ImportStrategy::Managed,
        )
        .unwrap();

        assert_eq!(report1.collection_id, Some(collection_id));

        let playlist_entities: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM source_ref WHERE kind = 'playlist' AND external_id = 'PL123'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(playlist_entities, 1);
    }

    #[test]
    fn import_collection_merges_playlist_duplicates_across_source_keys() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let conn = vault.connection();
        let now = Utc::now().to_rfc3339();

        let legacy_id = Uuid::new_v4();
        db::insert_entity(conn, legacy_id, None, None, 0, "active", &now, None).unwrap();
        db::insert_collection(
            conn,
            legacy_id,
            "youtube_playlist",
            "Legacy playlist",
        )
        .unwrap();
        db::insert_source_ref(
            conn,
            Uuid::new_v4(),
            legacy_id,
            "yt-dlp",
            "playlist",
            "PLdup",
            Some("https://youtube.com/playlist?list=PLdup"),
            "live",
        )
        .unwrap();
        let legacy_video = db::resolve_or_create_entity_by_source_ref(
            conn,
            "youtube",
            "video",
            "vid-legacy",
            None,
        )
        .unwrap();
        db::link_collection_member_direct(conn, legacy_id, legacy_video, 0).unwrap();

        let duplicate_id = Uuid::new_v4();
        db::insert_entity(conn, duplicate_id, None, None, 0, "active", &now, None).unwrap();
        db::insert_collection(
            conn,
            duplicate_id,
            "youtube_playlist",
            "Duplicate playlist",
        )
        .unwrap();
        db::insert_source_ref(
            conn,
            Uuid::new_v4(),
            duplicate_id,
            "youtube",
            "playlist",
            "PLdup",
            Some("https://youtube.com/playlist?list=PLdup"),
            "live",
        )
        .unwrap();
        let duplicate_video = db::resolve_or_create_entity_by_source_ref(
            conn,
            "youtube",
            "video",
            "vid-new",
            None,
        )
        .unwrap();
        db::link_collection_member_direct(conn, duplicate_id, duplicate_video, 1).unwrap();

        let staging = dir.path().join("staging/resync");
        fs::create_dir_all(&staging).unwrap();
        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: Some("youtube".into()),
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: Some(ManifestCollection {
                collection_type: "youtube_playlist".into(),
                external_id: "PLdup".into(),
                url: "https://youtube.com/playlist?list=PLdup".into(),
                title: "Merged playlist".into(),
            }),
            channels: vec![],
            items: vec![],
            membership: vec![
                archiveos_contract::ManifestMembership {
                    external_id: "vid-legacy".into(),
                    position: 0,
                    kind: "video".into(),
                    url: None,
                },
                archiveos_contract::ManifestMembership {
                    external_id: "vid-new".into(),
                    position: 1,
                    kind: "video".into(),
                    url: None,
                },
            ],
            relations: vec![],
        };

        let report = import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();
        assert_eq!(report.collection_id, Some(legacy_id));

        let members: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM collection_member WHERE collection_id = ?1 AND status = 'active'",
                [legacy_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(members, 2);

        let playlist_entities: i32 = conn
            .query_row(
                "SELECT COUNT(DISTINCT entity_id) FROM source_ref WHERE kind = 'playlist' AND external_id = 'PLdup'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(playlist_entities, 1);
    }

    #[test]
    fn import_collection_membership_creates_logical_video_entities_without_files() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        fs::create_dir_all(&staging).unwrap();

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: Some(ManifestCollection {
                collection_type: "youtube_playlist".into(),
                external_id: "PL123".into(),
                url: "https://youtube.com/playlist?list=PL123".into(),
                title: "My List".into(),
            }),
            channels: vec![],
            items: vec![],
            membership: vec![
                archiveos_contract::ManifestMembership {
                    external_id: "ext-0".into(),
                    position: 0,
                    kind: "video".into(),
                    url: None,
                },
                archiveos_contract::ManifestMembership {
                    external_id: "ext-1".into(),
                    position: 1,
                    kind: "video".into(),
                    url: None,
                },
            ],
            relations: vec![],
        };

        import(
            &vault,
            &staging,
            Some(&manifest),
            ImportStrategy::Managed,
        )
        .unwrap();

        let video_entities: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM source_ref WHERE source = 'youtube' AND kind = 'video'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(video_entities, 2);

        let active_members: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM collection_member WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active_members, 2);
    }

    #[test]
    fn import_same_video_with_different_bytes_creates_multiple_assets() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();

        let staging1 = dir.path().join("staging/job-1");
        let file1 = staging1.join("files/video-720.mp4");
        fs::create_dir_all(file1.parent().unwrap()).unwrap();
        fs::write(&file1, b"video-720").unwrap();
        import(
            &vault,
            &staging1,
            Some(&sample_manifest("files/video-720.mp4")),
            ImportStrategy::Managed,
        )
        .unwrap();

        let staging2 = dir.path().join("staging/job-2");
        let file2 = staging2.join("files/video-1080.mp4");
        fs::create_dir_all(file2.parent().unwrap()).unwrap();
        fs::write(&file2, b"video-1080").unwrap();
        import(
            &vault,
            &staging2,
            Some(&sample_manifest("files/video-1080.mp4")),
            ImportStrategy::Managed,
        )
        .unwrap();

        let video_entities: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM source_ref WHERE source = 'youtube' AND kind = 'video' AND external_id = 'vid123'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(video_entities, 1);

        let assets: i32 = vault
            .connection()
            .query_row("SELECT COUNT(*) FROM entity_asset WHERE kind = 'video'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(assets, 2);
    }

    #[test]
    fn import_playlist_resync_marks_absent_members_removed_from_source() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging0 = dir.path().join("staging/job-0");
        fs::create_dir_all(&staging0).unwrap();

        let mut manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: Some(ManifestCollection {
                collection_type: "youtube_playlist".into(),
                external_id: "PL123".into(),
                url: "https://youtube.com/playlist?list=PL123".into(),
                title: "My List".into(),
            }),
            channels: vec![],
            items: vec![],
            membership: vec![
                archiveos_contract::ManifestMembership {
                    external_id: "ext-0".into(),
                    position: 0,
                    kind: "video".into(),
                    url: None,
                },
                archiveos_contract::ManifestMembership {
                    external_id: "ext-1".into(),
                    position: 1,
                    kind: "video".into(),
                    url: None,
                },
            ],
            relations: vec![],
        };
        import(&vault, &staging0, Some(&manifest), ImportStrategy::Managed).unwrap();

        let staging1 = dir.path().join("staging/job-1");
        fs::create_dir_all(&staging1).unwrap();
        manifest.membership = vec![archiveos_contract::ManifestMembership {
            external_id: "ext-0".into(),
            position: 0,
            kind: "video".into(),
            url: None,
        }];
        import(&vault, &staging1, Some(&manifest), ImportStrategy::Managed).unwrap();

        let removed: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM collection_member WHERE status = 'removed_from_source'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(removed, 1);
    }

    #[test]
    fn import_channel_and_uploaded_by_relation() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        let file = staging.join("files/video.mp4");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"video-content").unwrap();

        let mut manifest = sample_manifest("files/video.mp4");
        manifest.channels.push(ManifestChannel {
            source: "youtube".into(),
            kind: "channel".into(),
            external_id: "UC123".into(),
            url: Some("https://youtube.com/channel/UC123".into()),
            metadata: Some(serde_json::json!({
                "title": "Test Channel",
                "verified": true
            })),
            metadata_by_provenance: Default::default(),
            avatar: None,
        });
        manifest.relations.push(ManifestRelation {
            source: "youtube".into(),
            from_kind: "video".into(),
            from_external_id: "vid123".into(),
            to_kind: "channel".into(),
            to_external_id: "UC123".into(),
            relation: "uploaded_by".into(),
        });

        import(
            &vault,
            &staging,
            Some(&manifest),
            ImportStrategy::Managed,
        )
        .unwrap();

        let channel_count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM source_ref WHERE kind = 'channel' AND external_id = 'UC123'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(channel_count, 1);

        let relation_count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entity_relation WHERE relation = 'uploaded_by'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(relation_count, 1);
    }

    #[test]
    fn import_channel_avatar_asset_on_channel_entity() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        let avatar = staging.join("files/UC123.jpg");
        fs::create_dir_all(avatar.parent().unwrap()).unwrap();
        fs::write(&avatar, b"avatar-bytes").unwrap();

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![ManifestChannel {
                source: "youtube".into(),
                kind: "channel".into(),
                external_id: "UC123".into(),
                url: Some("https://youtube.com/channel/UC123".into()),
                metadata: Some(serde_json::json!({ "title": "Avatar Channel" })),
                metadata_by_provenance: Default::default(),
                avatar: Some(ManifestChannelAvatar {
                    path: "files/UC123.jpg".into(),
                    source_url: Some("https://example.com/avatar.jpg".into()),
                }),
            }],
            items: vec![],
            membership: vec![],
            relations: vec![],
        };

        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();

        let channel_entity_id: String = vault
            .connection()
            .query_row(
                "SELECT entity_id FROM source_ref WHERE kind = 'channel' AND external_id = 'UC123'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let avatar_count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entity_asset
                 WHERE entity_id = ?1 AND kind = 'avatar' AND status = 'present'",
                [&channel_entity_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(avatar_count, 1);
    }

    #[test]
    fn import_video_and_thumbnail_relation() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        let video = staging.join("files/video.mp4");
        let thumb = staging.join("files/vid123.webp");
        fs::create_dir_all(video.parent().unwrap()).unwrap();
        fs::write(&video, b"video-content").unwrap();
        fs::write(&thumb, b"webp-thumb").unwrap();

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![
                ManifestItem {
                    path: "files/video.mp4".into(),
                    sha256: None,
                    status: "complete".into(),
                    source_ref: Some(ManifestSourceRef {
                        source: "youtube".into(),
                        kind: "video".into(),
                        external_id: "vid123".into(),
                        url: Some("https://example.com/watch?v=vid123".into()),
                    }),
                    metadata: Some(serde_json::json!({
                        "title": "Test Video",
                        "thumbnail_external_id": "vid123:thumbnail"
                    })),
                    metadata_by_provenance: {
                        let mut map = std::collections::BTreeMap::new();
                        map.insert(
                            "yt-dlp".into(),
                            serde_json::json!({
                                "title": "Test Video",
                                "description": "From YouTube"
                            }),
                        );
                        map.insert(
                            "ffprobe".into(),
                            serde_json::json!({
                                "actual_height": 1080,
                                "video_codec": "h264"
                            }),
                        );
                        map.insert(
                            "archiveos".into(),
                            serde_json::json!({
                                "thumbnail_external_id": "vid123:thumbnail"
                            }),
                        );
                        map
                    },
                    assets: vec![],
                },
                ManifestItem {
                    path: "files/vid123.webp".into(),
                    sha256: None,
                    status: "complete".into(),
                    source_ref: Some(ManifestSourceRef {
                        source: "youtube".into(),
                        kind: "thumbnail".into(),
                        external_id: "vid123:thumbnail".into(),
                        url: Some("https://example.com/thumb.webp".into()),
                    }),
                    metadata: Some(serde_json::json!({
                        "title": "Test Video thumbnail",
                        "thumbnail_for": "vid123"
                    })),
                    metadata_by_provenance: {
                        let mut map = std::collections::BTreeMap::new();
                        map.insert(
                            "archiveos".into(),
                            serde_json::json!({
                                "entity_role": "supporting",
                                "asset_role": "thumbnail",
                                "visibility": "hidden",
                                "thumbnail_for": "vid123"
                            }),
                        );
                        map
                    },
                    assets: vec![],
                },
            ],
            membership: vec![],
            relations: vec![ManifestRelation {
                source: "youtube".into(),
                from_kind: "video".into(),
                from_external_id: "vid123".into(),
                to_kind: "thumbnail".into(),
                to_external_id: "vid123:thumbnail".into(),
                relation: "thumbnail".into(),
            }],
        };

        import(
            &vault,
            &staging,
            Some(&manifest),
            ImportStrategy::Managed,
        )
        .unwrap();

        let video_count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM source_ref WHERE kind = 'video' AND external_id = 'vid123'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(video_count, 1);

        let thumb_count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM source_ref WHERE kind = 'thumbnail' AND external_id = 'vid123:thumbnail'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(thumb_count, 1);

        let relation_count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entity_relation WHERE relation = 'thumbnail'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(relation_count, 1);

        let thumb_ref: String = vault
            .connection()
            .query_row(
                "SELECT value FROM metadata WHERE key = 'thumbnail_external_id' AND provenance = 'archiveos'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(thumb_ref, "vid123:thumbnail");

        let hidden: String = vault
            .connection()
            .query_row(
                "SELECT value FROM metadata WHERE key = 'visibility' AND provenance = 'archiveos'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hidden, "hidden");

        let codec_prov: String = vault
            .connection()
            .query_row(
                "SELECT provenance FROM metadata WHERE key = 'video_codec'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(codec_prov, "ffprobe");

        let thumb_assets: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entity_asset WHERE kind = 'thumbnail' AND role = 'supporting' AND status = 'present'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(thumb_assets, 1);
    }

    #[test]
    fn import_thumbnail_reimport_supersedes_previous_asset() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        let thumb_v1 = staging.join("files/vid123.webp");
        let thumb_v2 = staging.join("files/vid123.jpg");
        fs::create_dir_all(thumb_v1.parent().unwrap()).unwrap();
        fs::write(&thumb_v1, b"small-thumb").unwrap();
        fs::write(&thumb_v2, b"much-larger-thumbnail-bytes").unwrap();

        let mut manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![ManifestItem {
                path: "files/vid123.webp".into(),
                sha256: None,
                status: "complete".into(),
                source_ref: Some(ManifestSourceRef {
                    source: "youtube".into(),
                    kind: "thumbnail".into(),
                    external_id: "vid123:thumbnail".into(),
                    url: Some("https://example.com/thumb.webp".into()),
                }),
                metadata: None,
                metadata_by_provenance: Default::default(),
                assets: vec![],
            }],
            membership: vec![],
            relations: vec![],
        };

        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();
        manifest.items[0].path = "files/vid123.jpg".into();
        fs::create_dir_all(thumb_v2.parent().unwrap()).unwrap();
        fs::write(&thumb_v2, b"much-larger-thumbnail-bytes").unwrap();
        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();

        let present: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entity_asset ea
                 JOIN source_ref sr ON sr.entity_id = ea.entity_id
                 WHERE sr.external_id = 'vid123:thumbnail' AND ea.kind = 'thumbnail'
                   AND ea.status = 'present'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(present, 1);

        let size: i64 = vault
            .connection()
            .query_row(
                "SELECT ea.size FROM entity_asset ea
                 JOIN source_ref sr ON sr.entity_id = ea.entity_id
                 WHERE sr.external_id = 'vid123:thumbnail' AND ea.kind = 'thumbnail'
                   AND ea.status = 'present'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(size > 10);
    }

    #[test]
    fn superseded_thumbnail_blob_removed_by_gc() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        let thumb_v1 = staging.join("files/vid123.webp");
        let thumb_v2 = staging.join("files/vid123.jpg");
        fs::create_dir_all(thumb_v1.parent().unwrap()).unwrap();
        fs::write(&thumb_v1, b"small-thumb").unwrap();
        fs::write(&thumb_v2, b"much-larger-thumbnail-bytes").unwrap();

        let mut manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![ManifestItem {
                path: "files/vid123.webp".into(),
                sha256: None,
                status: "complete".into(),
                source_ref: Some(ManifestSourceRef {
                    source: "youtube".into(),
                    kind: "thumbnail".into(),
                    external_id: "vid123:thumbnail".into(),
                    url: Some("https://example.com/thumb.webp".into()),
                }),
                metadata: None,
                metadata_by_provenance: Default::default(),
                assets: vec![],
            }],
            membership: vec![],
            relations: vec![],
        };

        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();
        manifest.items[0].path = "files/vid123.jpg".into();
        fs::create_dir_all(thumb_v2.parent().unwrap()).unwrap();
        fs::write(&thumb_v2, b"much-larger-thumbnail-bytes").unwrap();
        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();

        let old_hash: String = vault
            .connection()
            .query_row(
                "SELECT content_hash FROM entity_asset
                 WHERE kind = 'thumbnail' AND status = 'missing'
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let new_hash: String = vault
            .connection()
            .query_row(
                "SELECT content_hash FROM entity_asset
                 WHERE kind = 'thumbnail' AND status = 'present'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let old_blob = crate::layout::blob_path(vault.root(), &old_hash, ".webp")
            .or_else(|_| crate::layout::find_blob_path(vault.root(), &old_hash, None, Some("image/webp")))
            .unwrap();
        let new_blob = crate::layout::find_blob_path(
            vault.root(),
            &new_hash,
            Some(".jpg"),
            Some("image/jpeg"),
        )
        .unwrap();
        assert!(old_blob.exists());
        assert!(new_blob.exists());

        let report = vault.reconcile_blobs(false, 0).unwrap();
        assert_eq!(report.removed, 1);
        assert!(!old_blob.exists());
        assert!(new_blob.exists());
    }

    #[test]
    fn import_private_video_sets_source_status_without_asset() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        fs::create_dir_all(&staging).unwrap();

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![ManifestItem {
                path: "files/private.mp4".into(),
                sha256: None,
                status: "private".into(),
                source_ref: Some(ManifestSourceRef {
                    source: "youtube".into(),
                    kind: "video".into(),
                    external_id: "private-vid".into(),
                    url: Some("https://example.com/watch?v=private-vid".into()),
                }),
                metadata: Some(serde_json::json!({ "title": "Private Video" })),
                metadata_by_provenance: Default::default(),
                assets: vec![],
            }],
            membership: vec![],
            relations: vec![],
        };

        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();

        let source_status: String = vault
            .connection()
            .query_row(
                "SELECT status FROM source_ref WHERE external_id = 'private-vid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_status, "private");

        let assets: i32 = vault
            .connection()
            .query_row("SELECT COUNT(*) FROM entity_asset", [], |row| row.get(0))
            .unwrap();
        assert_eq!(assets, 0);
    }

    #[test]
    fn import_dead_video_sets_source_status_without_asset() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        fs::create_dir_all(&staging).unwrap();

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![ManifestItem {
                path: "files/dead.mp4".into(),
                sha256: None,
                status: "dead".into(),
                source_ref: Some(ManifestSourceRef {
                    source: "youtube".into(),
                    kind: "video".into(),
                    external_id: "dead-vid".into(),
                    url: Some("https://example.com/watch?v=dead-vid".into()),
                }),
                metadata: None,
                metadata_by_provenance: Default::default(),
                assets: vec![],
            }],
            membership: vec![],
            relations: vec![],
        };

        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();

        let source_status: String = vault
            .connection()
            .query_row(
                "SELECT status FROM source_ref WHERE external_id = 'dead-vid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_status, "dead");
    }

    #[test]
    fn collection_resync_does_not_reactivate_user_removed_member() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging0 = dir.path().join("staging/job-0");
        fs::create_dir_all(&staging0).unwrap();

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: Some(ManifestCollection {
                collection_type: "youtube_playlist".into(),
                external_id: "PL123".into(),
                url: "https://youtube.com/playlist?list=PL123".into(),
                title: "My List".into(),
            }),
            channels: vec![],
            items: vec![],
            membership: vec![
                archiveos_contract::ManifestMembership {
                    external_id: "ext-0".into(),
                    position: 0,
                    kind: "video".into(),
                    url: None,
                },
                archiveos_contract::ManifestMembership {
                    external_id: "ext-1".into(),
                    position: 1,
                    kind: "video".into(),
                    url: None,
                },
            ],
            relations: vec![],
        };
        let report = import(&vault, &staging0, Some(&manifest), ImportStrategy::Managed).unwrap();
        let collection_id = report.collection_id.unwrap();

        let removed_entity: Uuid = vault
            .connection()
            .query_row(
                "SELECT entity_id FROM source_ref WHERE external_id = 'ext-1'",
                [],
                |row| {
                    let id: String = row.get(0)?;
                    Ok(Uuid::parse_str(&id).unwrap())
                },
            )
            .unwrap();
        vault
            .remove_collection_member(collection_id, removed_entity)
            .unwrap();

        let staging1 = dir.path().join("staging/job-1");
        fs::create_dir_all(&staging1).unwrap();
        import(&vault, &staging1, Some(&manifest), ImportStrategy::Managed).unwrap();

        let status: String = vault
            .connection()
            .query_row(
                "SELECT status FROM collection_member WHERE collection_id = ?1 AND entity_id = ?2",
                params![collection_id.to_string(), removed_entity.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "user_removed");

        let active: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM collection_member WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active, 1);
    }

    #[test]
    fn remove_collection_member_marks_user_removed() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-0");
        fs::create_dir_all(&staging).unwrap();

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: Some(ManifestCollection {
                collection_type: "youtube_playlist".into(),
                external_id: "PL999".into(),
                url: "https://youtube.com/playlist?list=PL999".into(),
                title: "List".into(),
            }),
            channels: vec![],
            items: vec![],
            membership: vec![archiveos_contract::ManifestMembership {
                external_id: "ext-x".into(),
                position: 0,
                kind: "video".into(),
                url: None,
            }],
            relations: vec![],
        };
        let report = import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();
        let collection_id = report.collection_id.unwrap();
        let entity_id: Uuid = vault
            .connection()
            .query_row(
                "SELECT entity_id FROM source_ref WHERE external_id = 'ext-x'",
                [],
                |row| {
                    let id: String = row.get(0)?;
                    Ok(Uuid::parse_str(&id).unwrap())
                },
            )
            .unwrap();

        vault.remove_collection_member(collection_id, entity_id).unwrap();

        let status: String = vault
            .connection()
            .query_row(
                "SELECT status FROM collection_member WHERE collection_id = ?1 AND entity_id = ?2",
                params![collection_id.to_string(), entity_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "user_removed");
    }

    #[test]
    fn import_metadata_by_provenance_writes_explicit_labels() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        let file = staging.join("files/video.mp4");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"video-content").unwrap();

        let mut groups = std::collections::BTreeMap::new();
        groups.insert("ffprobe".into(), serde_json::json!({ "actual_height": 720 }));
        groups.insert("archiveos".into(), serde_json::json!({ "thumbnail_external_id": "vid:thumbnail" }));

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: None,
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![ManifestItem {
                path: "files/video.mp4".into(),
                sha256: None,
                status: "complete".into(),
                source_ref: Some(ManifestSourceRef {
                    source: "youtube".into(),
                    kind: "video".into(),
                    external_id: "vid".into(),
                    url: None,
                }),
                metadata: Some(serde_json::json!({ "title": "Legacy" })),
                metadata_by_provenance: groups,
                assets: vec![],
            }],
            membership: vec![],
            relations: vec![],
        };

        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();

        let title_prov: String = vault
            .connection()
            .query_row(
                "SELECT provenance FROM metadata WHERE key = 'title'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(title_prov, "yt-dlp");

        let height_prov: String = vault
            .connection()
            .query_row(
                "SELECT provenance FROM metadata WHERE key = 'actual_height'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(height_prov, "ffprobe");
    }

    #[test]
    fn import_skips_null_and_empty_optional_metadata() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        let file = staging.join("files/video.mp4");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"video-content").unwrap();

        let mut groups = std::collections::BTreeMap::new();
        groups.insert(
            "yt-dlp".into(),
            serde_json::json!({
                "title": "PH Title",
                "description": null
            }),
        );

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: Some("pornhub".into()),
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![ManifestItem {
                path: "files/video.mp4".into(),
                sha256: None,
                status: "complete".into(),
                source_ref: Some(ManifestSourceRef {
                    source: "pornhub".into(),
                    kind: "video".into(),
                    external_id: "ph123".into(),
                    url: Some("https://www.pornhub.com/view_video.php?viewkey=ph123".into()),
                }),
                metadata: None,
                metadata_by_provenance: groups,
                assets: vec![],
            }],
            membership: vec![],
            relations: vec![],
        };

        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();

        let description_count: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM metadata WHERE key = 'description'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(description_count, 0);
    }

    #[test]
    fn import_catalog_assets_creates_remote_subtitle_track() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        let file = staging.join("files/video.mp4");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"video-content").unwrap();

        let mut subtitle_meta = std::collections::BTreeMap::new();
        subtitle_meta.insert("language".into(), serde_json::json!("en"));
        subtitle_meta.insert("ext".into(), serde_json::json!("vtt"));
        subtitle_meta.insert("caption_kind".into(), serde_json::json!("manual"));
        subtitle_meta.insert(
            "source_url".into(),
            serde_json::json!("https://example.com/subs/en.vtt"),
        );

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: Some("pornhub".into()),
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![ManifestItem {
                path: "files/video.mp4".into(),
                sha256: None,
                status: "complete".into(),
                source_ref: Some(ManifestSourceRef {
                    source: "pornhub".into(),
                    kind: "video".into(),
                    external_id: "ph123".into(),
                    url: None,
                }),
                metadata: None,
                metadata_by_provenance: Default::default(),
                assets: vec![ManifestAssetCatalogEntry {
                    track_key: "subtitle:en:vtt:manual".into(),
                    role: "supporting".into(),
                    kind: "subtitle".into(),
                    status: "remote".into(),
                    storage_strategy: "remote".into(),
                    external_id: "ph123:subtitle:en:vtt:manual".into(),
                    metadata: subtitle_meta,
                }],
            }],
            membership: vec![],
            relations: vec![],
        };

        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();

        let remote_assets: i32 = vault
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM entity_asset WHERE kind = 'subtitle' AND status = 'remote'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remote_assets, 1);

        let language: String = vault
            .connection()
            .query_row(
                "SELECT m.value FROM entity_asset_metadata m
                 JOIN entity_asset a ON a.id = m.asset_id
                 WHERE m.key = 'language' AND a.kind = 'subtitle'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(language, "en");
    }

    #[test]
    fn import_manifest_source_identity_overrides_legacy_default() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let staging = dir.path().join("staging/job-1");
        fs::create_dir_all(&staging).unwrap();

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: Some("pornhub".into()),
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: Some(ManifestCollection {
                collection_type: "pornhub_playlist".into(),
                external_id: "pl1".into(),
                url: "https://www.pornhub.com/playlist/pl1".into(),
                title: "List".into(),
            }),
            channels: vec![],
            items: vec![],
            membership: vec![archiveos_contract::ManifestMembership {
                external_id: "vid1".into(),
                position: 0,
                kind: "video".into(),
                url: Some("https://www.pornhub.com/view_video.php?viewkey=vid1".into()),
            }],
            relations: vec![],
        };

        import(&vault, &staging, Some(&manifest), ImportStrategy::Managed).unwrap();

        let source: String = vault
            .connection()
            .query_row(
                "SELECT source FROM source_ref WHERE external_id = 'vid1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source, "pornhub");
    }
}
