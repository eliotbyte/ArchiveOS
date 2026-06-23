use std::collections::{BTreeMap, HashMap};

use archiveos_contract::{ManifestAssetCatalogEntry, MetadataEntry, VaultError};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityAsset {
    pub id: Uuid,
    pub entity_id: Uuid,
    pub role: String,
    pub kind: String,
    pub content_hash: Option<String>,
    pub mime: Option<String>,
    pub size: i64,
    pub ext: Option<String>,
    pub status: String,
    pub storage_strategy: String,
    pub path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

use crate::metadata;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityAssetWithMetadata {
    pub asset: EntityAsset,
    pub metadata_entries: Vec<MetadataEntry>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct UpsertAssetInput<'a> {
    pub entity_id: Uuid,
    pub role: &'a str,
    pub kind: &'a str,
    pub content_hash: Option<&'a str>,
    pub mime: Option<&'a str>,
    pub size: u64,
    pub ext: Option<&'a str>,
    pub status: &'a str,
    pub storage_strategy: &'a str,
    pub path: Option<&'a str>,
}

pub fn upsert_asset(conn: &Connection, input: &UpsertAssetInput<'_>) -> Result<Uuid, VaultError> {
    if let Some(content_hash) = input.content_hash {
        if let Some(existing) = find_asset_by_entity_kind_hash(
            conn,
            input.entity_id,
            input.kind,
            content_hash,
        )? {
            update_asset_present(conn, existing, input)?;
            return Ok(existing);
        }
    }

    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO entity_asset (
            id, entity_id, role, kind, content_hash, mime, size, ext, status,
            storage_strategy, path, created_at, updated_at, deleted_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, NULL)",
        params![
            id.to_string(),
            input.entity_id.to_string(),
            input.role,
            input.kind,
            input.content_hash,
            input.mime,
            i64::try_from(input.size).unwrap_or(i64::MAX),
            input.ext,
            input.status,
            input.storage_strategy,
            input.path,
            now,
            now,
        ],
    )
    .map_err(db_err)?;
    Ok(id)
}

pub fn upsert_catalog_asset(
    conn: &Connection,
    entity_id: Uuid,
    entry: &ManifestAssetCatalogEntry,
) -> Result<Uuid, VaultError> {
    if let Some(existing) = find_asset_by_track_key(conn, entity_id, &entry.track_key)? {
        update_catalog_asset(conn, existing, entry)?;
        return Ok(existing);
    }

    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO entity_asset (
            id, entity_id, role, kind, content_hash, mime, size, ext, status,
            storage_strategy, path, created_at, updated_at, deleted_at
         ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, 0, NULL, ?5, ?6, NULL, ?7, ?7, NULL)",
        params![
            id.to_string(),
            entity_id.to_string(),
            entry.role,
            entry.kind,
            entry.status,
            entry.storage_strategy,
            now,
        ],
    )
    .map_err(db_err)?;
    upsert_asset_metadata(conn, id, "track_key", &entry.track_key, "archiveos")?;
    upsert_asset_metadata(conn, id, "external_id", &entry.external_id, "archiveos")?;
    upsert_asset_metadata_map(conn, id, &entry.metadata, "yt-dlp")?;
    Ok(id)
}

pub fn upsert_asset_metadata(
    conn: &Connection,
    asset_id: Uuid,
    key: &str,
    value: &str,
    provenance: &str,
) -> Result<(), VaultError> {
    conn.execute(
        "INSERT INTO entity_asset_metadata (asset_id, key, value, provenance)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(asset_id, key, provenance) DO UPDATE SET value = excluded.value",
        params![asset_id.to_string(), key, value, provenance],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn upsert_asset_metadata_map(
    conn: &Connection,
    asset_id: Uuid,
    metadata: &BTreeMap<String, Value>,
    provenance: &str,
) -> Result<(), VaultError> {
    for (key, value) in metadata {
        if value.is_null() {
            continue;
        }
        upsert_asset_metadata(conn, asset_id, key, &metadata_value_to_string(value), provenance)?;
    }
    Ok(())
}

pub fn mark_catalog_asset_present(
    conn: &Connection,
    asset_id: Uuid,
    input: &UpsertAssetInput<'_>,
) -> Result<(), VaultError> {
    conn.execute(
        "UPDATE entity_asset
         SET role = ?1, kind = ?2, content_hash = ?3, mime = ?4, size = ?5, ext = ?6,
             status = ?7, storage_strategy = ?8, path = ?9, updated_at = ?10, deleted_at = NULL
         WHERE id = ?11",
        params![
            input.role,
            input.kind,
            input.content_hash,
            input.mime,
            i64::try_from(input.size).unwrap_or(i64::MAX),
            input.ext,
            input.status,
            input.storage_strategy,
            input.path,
            Utc::now().to_rfc3339(),
            asset_id.to_string(),
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

fn find_asset_by_track_key(
    conn: &Connection,
    entity_id: Uuid,
    track_key: &str,
) -> Result<Option<Uuid>, VaultError> {
    let result = conn
        .query_row(
            "SELECT a.id FROM entity_asset a
             JOIN entity_asset_metadata m ON m.asset_id = a.id
             WHERE a.entity_id = ?1 AND m.key = 'track_key' AND m.value = ?2
               AND m.provenance = 'archiveos'",
            params![entity_id.to_string(), track_key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_err)?;
    result
        .map(|id| {
            Uuid::parse_str(&id).map_err(|err| VaultError::InvalidLayout {
                detail: err.to_string(),
            })
        })
        .transpose()
}

fn update_catalog_asset(
    conn: &Connection,
    asset_id: Uuid,
    entry: &ManifestAssetCatalogEntry,
) -> Result<(), VaultError> {
    conn.execute(
        "UPDATE entity_asset
         SET role = ?1, kind = ?2, status = ?3, storage_strategy = ?4, updated_at = ?5
         WHERE id = ?6",
        params![
            entry.role,
            entry.kind,
            entry.status,
            entry.storage_strategy,
            Utc::now().to_rfc3339(),
            asset_id.to_string(),
        ],
    )
    .map_err(db_err)?;
    upsert_asset_metadata(conn, asset_id, "external_id", &entry.external_id, "archiveos")?;
    upsert_asset_metadata_map(conn, asset_id, &entry.metadata, "yt-dlp")?;
    Ok(())
}

fn metadata_value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn asset_belongs_to_entity(asset: &EntityAsset, entity_id: Uuid) -> bool {
    asset.entity_id == entity_id
}

pub fn list_asset_metadata(
    conn: &Connection,
    asset_id: Uuid,
) -> Result<Vec<MetadataEntry>, VaultError> {
    let mut stmt = conn
        .prepare(
            "SELECT key, value, provenance FROM entity_asset_metadata WHERE asset_id = ?1
             ORDER BY key, provenance",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([asset_id.to_string()], |row| {
            Ok(MetadataEntry {
                key: row.get(0)?,
                value: row.get(1)?,
                provenance: row.get(2)?,
            })
        })
        .map_err(db_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

pub fn list_assets_with_metadata(
    conn: &Connection,
    entity_id: Uuid,
) -> Result<Vec<EntityAssetWithMetadata>, VaultError> {
    let assets = list_assets(conn, entity_id)?;
    let mut result = Vec::with_capacity(assets.len());
    for asset in assets {
        let metadata_entries = list_asset_metadata(conn, asset.id)?;
        let metadata = metadata::flatten_metadata(&metadata_entries);
        result.push(EntityAssetWithMetadata {
            asset,
            metadata_entries,
            metadata,
        });
    }
    Ok(result)
}

pub fn list_assets(conn: &Connection, entity_id: Uuid) -> Result<Vec<EntityAsset>, VaultError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, entity_id, role, kind, content_hash, mime, size, ext, status,
                    storage_strategy, path, created_at, updated_at, deleted_at
             FROM entity_asset
             WHERE entity_id = ?1
             ORDER BY created_at ASC",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([entity_id.to_string()], map_asset)
        .map_err(db_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

pub fn get_asset(conn: &Connection, asset_id: Uuid) -> Result<EntityAsset, VaultError> {
    conn.query_row(
        "SELECT id, entity_id, role, kind, content_hash, mime, size, ext, status,
                storage_strategy, path, created_at, updated_at, deleted_at
         FROM entity_asset
         WHERE id = ?1",
        [asset_id.to_string()],
        map_asset,
    )
    .optional()
    .map_err(db_err)?
    .ok_or(VaultError::NotFound)
}

pub fn asset_statuses(conn: &Connection, entity_id: Uuid) -> Result<Vec<String>, VaultError> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT status FROM entity_asset
             WHERE entity_id = ?1
             ORDER BY status",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([entity_id.to_string()], |row| row.get(0))
        .map_err(db_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

pub fn has_present_asset(conn: &Connection, entity_id: Uuid) -> Result<bool, VaultError> {
    let found: i32 = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM entity_asset
                WHERE entity_id = ?1 AND status = 'present'
            )",
            [entity_id.to_string()],
            |row| row.get(0),
        )
        .map_err(db_err)?;
    Ok(found != 0)
}

/// Marks other present assets of the same kind on an entity as missing after a replacement import.
pub fn supersede_present_assets(
    conn: &Connection,
    entity_id: Uuid,
    kind: &str,
    keep_asset_id: Uuid,
) -> Result<(), VaultError> {
    conn.execute(
        "UPDATE entity_asset
         SET status = 'missing', updated_at = ?1, deleted_at = NULL
         WHERE entity_id = ?2 AND kind = ?3 AND status = 'present' AND id != ?4",
        params![
            Utc::now().to_rfc3339(),
            entity_id.to_string(),
            kind,
            keep_asset_id.to_string(),
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn mark_asset_status(
    conn: &Connection,
    asset_id: Uuid,
    status: &str,
    deleted_at: Option<&str>,
) -> Result<(), VaultError> {
    let changed = conn
        .execute(
            "UPDATE entity_asset
             SET status = ?1, updated_at = ?2, deleted_at = COALESCE(?3, deleted_at)
             WHERE id = ?4",
            params![status, Utc::now().to_rfc3339(), deleted_at, asset_id.to_string()],
        )
        .map_err(db_err)?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

pub fn mark_entity_assets_deleted(conn: &Connection, entity_id: Uuid) -> Result<(), VaultError> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE entity_asset
         SET status = 'deleted_by_user', updated_at = ?1, deleted_at = ?1
         WHERE entity_id = ?2 AND status != 'deleted_by_user'",
        params![now, entity_id.to_string()],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn find_primary_asset(
    conn: &Connection,
    entity_id: Uuid,
) -> Result<Option<EntityAsset>, VaultError> {
    conn.query_row(
        "SELECT id, entity_id, role, kind, content_hash, mime, size, ext, status,
                storage_strategy, path, created_at, updated_at, deleted_at
         FROM entity_asset
         WHERE entity_id = ?1 AND role = 'primary'
         ORDER BY created_at ASC
         LIMIT 1",
        [entity_id.to_string()],
        map_asset,
    )
    .optional()
    .map_err(db_err)
}

pub fn find_asset_by_preview_role(
    conn: &Connection,
    entity_id: Uuid,
    preview_role: &str,
) -> Result<Option<EntityAsset>, VaultError> {
    conn.query_row(
        "SELECT ea.id, ea.entity_id, ea.role, ea.kind, ea.content_hash, ea.mime, ea.size, ea.ext,
                ea.status, ea.storage_strategy, ea.path, ea.created_at, ea.updated_at, ea.deleted_at
         FROM entity_asset ea
         JOIN entity_asset_metadata eam ON eam.asset_id = ea.id
         WHERE ea.entity_id = ?1 AND eam.key = 'preview_role' AND eam.value = ?2
         ORDER BY ea.updated_at DESC
         LIMIT 1",
        params![entity_id.to_string(), preview_role],
        map_asset,
    )
    .optional()
    .map_err(db_err)
}

pub fn upsert_preview_asset(
    conn: &Connection,
    entity_id: Uuid,
    preview_role: &str,
    _kind: &str,
    input: &UpsertAssetInput<'_>,
) -> Result<Uuid, VaultError> {
    if let Some(existing) = find_asset_by_preview_role(conn, entity_id, preview_role)? {
        update_asset_present(conn, existing.id, input)?;
        return Ok(existing.id);
    }
    upsert_asset(conn, input)
}

fn find_asset_by_entity_kind_hash(
    conn: &Connection,
    entity_id: Uuid,
    kind: &str,
    content_hash: &str,
) -> Result<Option<Uuid>, VaultError> {
    let result = conn
        .query_row(
            "SELECT id FROM entity_asset
             WHERE entity_id = ?1 AND kind = ?2 AND content_hash = ?3",
            params![entity_id.to_string(), kind, content_hash],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(db_err)?;
    result
        .map(|id| Uuid::parse_str(&id).map_err(|err| VaultError::InvalidLayout {
            detail: err.to_string(),
        }))
        .transpose()
}

fn update_asset_present(
    conn: &Connection,
    asset_id: Uuid,
    input: &UpsertAssetInput<'_>,
) -> Result<(), VaultError> {
    conn.execute(
        "UPDATE entity_asset
         SET role = ?1, mime = ?2, size = ?3, ext = ?4, status = ?5,
             storage_strategy = ?6, path = ?7, updated_at = ?8, deleted_at = NULL
         WHERE id = ?9",
        params![
            input.role,
            input.mime,
            i64::try_from(input.size).unwrap_or(i64::MAX),
            input.ext,
            input.status,
            input.storage_strategy,
            input.path,
            Utc::now().to_rfc3339(),
            asset_id.to_string(),
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

fn map_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntityAsset> {
    Ok(EntityAsset {
        id: parse_uuid(row, 0)?,
        entity_id: parse_uuid(row, 1)?,
        role: row.get(2)?,
        kind: row.get(3)?,
        content_hash: row.get(4)?,
        mime: row.get(5)?,
        size: row.get(6)?,
        ext: row.get(7)?,
        status: row.get(8)?,
        storage_strategy: row.get(9)?,
        path: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
        deleted_at: row.get(13)?,
    })
}

fn parse_uuid(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&row.get::<_, String>(idx)?).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            Box::new(err),
        )
    })
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vault;
    use tempfile::tempdir;

    #[test]
    fn list_asset_metadata_returns_track_fields() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let entity_id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();
        crate::import::db::insert_entity(conn, entity_id, None, None, 0, "active", &now, None)
            .unwrap();

        let asset_id = upsert_catalog_asset(
            conn,
            entity_id,
            &ManifestAssetCatalogEntry {
                track_key: "subtitle:en:vtt:manual".into(),
                role: "supporting".into(),
                kind: "subtitle".into(),
                status: "remote".into(),
                storage_strategy: "remote".into(),
                external_id: "vid:subtitle:en:vtt:manual".into(),
                metadata: BTreeMap::from([
                    ("language".into(), serde_json::json!("en")),
                    ("source_url".into(), serde_json::json!("https://example.com/en.vtt")),
                ]),
            },
        )
        .unwrap();

        let entries = list_asset_metadata(conn, asset_id).unwrap();
        assert!(entries.iter().any(|e| e.key == "language" && e.value == "en"));
        assert!(entries.iter().any(|e| e.key == "track_key"));

        let with_meta = list_assets_with_metadata(conn, entity_id).unwrap();
        assert_eq!(with_meta.len(), 1);
        assert_eq!(with_meta[0].metadata.get("language").map(String::as_str), Some("en"));
    }

    #[test]
    fn mark_catalog_asset_present_updates_remote_row() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let entity_id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();
        crate::import::db::insert_entity(conn, entity_id, None, None, 0, "active", &now, None)
            .unwrap();

        let asset_id = upsert_catalog_asset(
            conn,
            entity_id,
            &ManifestAssetCatalogEntry {
                track_key: "subtitle:en:vtt:manual".into(),
                role: "supporting".into(),
                kind: "subtitle".into(),
                status: "remote".into(),
                storage_strategy: "remote".into(),
                external_id: "vid:subtitle:en:vtt:manual".into(),
                metadata: Default::default(),
            },
        )
        .unwrap();

        mark_catalog_asset_present(
            conn,
            asset_id,
            &UpsertAssetInput {
                entity_id,
                role: "supporting",
                kind: "subtitle",
                content_hash: Some("abc123"),
                mime: Some("text/vtt"),
                size: 42,
                ext: Some(".vtt"),
                status: "present",
                storage_strategy: "managed",
                path: Some("/blobs/abc123.vtt"),
            },
        )
        .unwrap();

        let asset = get_asset(conn, asset_id).unwrap();
        assert_eq!(asset.status, "present");
        assert_eq!(asset.content_hash.as_deref(), Some("abc123"));
        assert_eq!(asset.mime.as_deref(), Some("text/vtt"));
    }
}
