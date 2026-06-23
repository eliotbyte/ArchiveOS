use std::collections::HashMap;

use std::path::Path;

use archiveos_contract::{EntityPreviewSummary, MetadataEntry, VaultError};
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

use crate::assets::{self, find_asset_by_preview_role};
use crate::layout::find_blob_path;
use crate::metadata;
use crate::preview::{
    ROLE_PREVIEW_IMAGE_LARGE, ROLE_PREVIEW_IMAGE_SMALL, ROLE_TIMELINE_MANIFEST,
    ROLE_TIMELINE_SPRITE,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityPreview {
    pub entity_id: Uuid,
    pub asset_id: Uuid,
    pub kind: String,
    pub preview_role: String,
    pub status: String,
}

impl From<EntityPreview> for EntityPreviewSummary {
    fn from(preview: EntityPreview) -> Self {
        Self {
            entity_id: preview.entity_id,
            asset_id: preview.asset_id,
            kind: preview.kind,
            preview_role: preview.preview_role,
            status: preview.status,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntityDetail {
    pub id: Uuid,
    pub content_hash: Option<String>,
    pub mime: Option<String>,
    pub size: i64,
    pub status: String,
    pub added_at: String,
    pub created_at: Option<String>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub metadata_entries: Vec<MetadataEntry>,
    pub assets: Vec<crate::assets::EntityAssetWithMetadata>,
    pub preview: Option<EntityPreview>,
    pub timeline_sprite: Option<EntityPreview>,
    pub timeline_manifest: Option<EntityPreview>,
}

pub fn get_entity(conn: &Connection, entity_id: Uuid) -> Result<EntityDetail, VaultError> {
    get_entity_at(conn, entity_id, None)
}

pub fn get_entity_at(
    conn: &Connection,
    entity_id: Uuid,
    vault_root: Option<&Path>,
) -> Result<EntityDetail, VaultError> {
    let row = conn
        .query_row(
            "SELECT id, content_hash, mime, size, status, added_at, created_at
             FROM entity WHERE id = ?1",
            [entity_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()
        .map_err(db_err)?;

    let Some((
        id,
        content_hash,
        mime,
        size,
        status,
        added_at,
        created_at,
    )) = row
    else {
        return Err(VaultError::NotFound);
    };

    let id = Uuid::parse_str(&id).map_err(|e| VaultError::InvalidLayout {
        detail: e.to_string(),
    })?;

    let tags = crate::tags::list_entity_tags(conn, id)?;
    let metadata_entries = load_metadata_entries(conn, id)?;
    let metadata = metadata::flatten_metadata(&metadata_entries);
    let assets = crate::assets::list_assets_with_metadata(conn, id)?;
    let preview = resolve_entity_preview_at(conn, id, vault_root)?;
    let timeline_sprite = resolve_timeline_sprite(conn, id)?;
    let timeline_manifest = resolve_timeline_manifest(conn, id)?;

    Ok(EntityDetail {
        id,
        content_hash,
        mime,
        size,
        status,
        added_at,
        created_at,
        tags,
        metadata,
        metadata_entries,
        assets,
        preview,
        timeline_sprite,
        timeline_manifest,
    })
}

pub fn resolve_entity_preview(
    conn: &Connection,
    entity_id: Uuid,
) -> Result<Option<EntityPreview>, VaultError> {
    resolve_entity_preview_at(conn, entity_id, None)
}

pub fn resolve_entity_preview_at(
    conn: &Connection,
    entity_id: Uuid,
    vault_root: Option<&Path>,
) -> Result<Option<EntityPreview>, VaultError> {
    let is_video = is_video_entity(conn, entity_id)?;

    if is_video {
        if let Some(preview) = resolve_source_thumbnail_preview_at(conn, entity_id, vault_root)? {
            return Ok(Some(preview));
        }
    }

    for role in [ROLE_PREVIEW_IMAGE_SMALL, ROLE_PREVIEW_IMAGE_LARGE] {
        if let Some(preview) = preview_from_role(conn, entity_id, role)? {
            if let Some(preview) = reconcile_preview(conn, vault_root, preview)? {
                return Ok(Some(preview));
            }
        }
    }

    if !is_video {
        resolve_source_thumbnail_preview_at(conn, entity_id, vault_root)
    } else {
        Ok(None)
    }
}

fn is_video_entity(conn: &Connection, entity_id: Uuid) -> Result<bool, VaultError> {
    if let Some(primary) = assets::find_primary_asset(conn, entity_id)? {
        if primary.kind == "video" {
            return Ok(true);
        }
        if primary
            .mime
            .as_deref()
            .is_some_and(|mime| mime.starts_with("video/"))
        {
            return Ok(true);
        }
    }

    let mime: Option<String> = conn
        .query_row(
            "SELECT mime FROM entity WHERE id = ?1",
            [entity_id.to_string()],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(db_err)?
        .flatten();
    Ok(mime
        .as_deref()
        .is_some_and(|mime| mime.starts_with("video/")))
}

pub fn resolve_timeline_sprite(
    conn: &Connection,
    entity_id: Uuid,
) -> Result<Option<EntityPreview>, VaultError> {
    preview_from_role(conn, entity_id, ROLE_TIMELINE_SPRITE)
}

pub fn resolve_timeline_manifest(
    conn: &Connection,
    entity_id: Uuid,
) -> Result<Option<EntityPreview>, VaultError> {
    preview_from_role(conn, entity_id, ROLE_TIMELINE_MANIFEST)
}

fn preview_from_role(
    conn: &Connection,
    entity_id: Uuid,
    preview_role: &str,
) -> Result<Option<EntityPreview>, VaultError> {
    let Some(asset) = find_asset_by_preview_role(conn, entity_id, preview_role)? else {
        return Ok(None);
    };
    Ok(Some(EntityPreview {
        entity_id,
        asset_id: asset.id,
        kind: asset.kind,
        preview_role: preview_role.to_string(),
        status: asset.status,
    }))
}

fn reconcile_preview(
    conn: &Connection,
    vault_root: Option<&Path>,
    preview: EntityPreview,
) -> Result<Option<EntityPreview>, VaultError> {
    if let Some(root) = vault_root {
        if !managed_asset_blob_exists(conn, root, preview.asset_id)? {
            let _ = assets::mark_asset_status(conn, preview.asset_id, "missing", None);
            return Ok(None);
        }
    }
    Ok(Some(preview))
}

fn managed_asset_blob_exists(
    conn: &Connection,
    vault_root: &Path,
    asset_id: Uuid,
) -> Result<bool, VaultError> {
    let asset = assets::get_asset(conn, asset_id)?;
    if asset.status != "present" {
        return Ok(false);
    }
    match asset.storage_strategy.as_str() {
        "managed" => {
            let Some(hash) = asset.content_hash.as_deref() else {
                return Ok(false);
            };
            Ok(find_blob_path(
                vault_root,
                hash,
                asset.ext.as_deref(),
                asset.mime.as_deref(),
            )
            .is_ok())
        }
        "reference" => Ok(asset
            .path
            .as_deref()
            .is_some_and(|path| Path::new(path).is_file())),
        _ => Ok(true),
    }
}

fn resolve_source_thumbnail_preview_at(
    conn: &Connection,
    entity_id: Uuid,
    vault_root: Option<&Path>,
) -> Result<Option<EntityPreview>, VaultError> {
    let thumb_external_id: Option<String> = conn
        .query_row(
            "SELECT value FROM metadata
             WHERE entity_id = ?1 AND key = 'thumbnail_external_id' AND provenance = 'archiveos'
             LIMIT 1",
            [entity_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;

    let Some(external_id) = thumb_external_id else {
        if let Some(primary) = assets::find_primary_asset(conn, entity_id)? {
            if primary.kind == "image" && primary.status == "present" {
                let preview = EntityPreview {
                    entity_id,
                    asset_id: primary.id,
                    kind: primary.kind,
                    preview_role: "primary_image".to_string(),
                    status: primary.status,
                };
                return reconcile_preview(conn, vault_root, preview);
            }
        }
        return Ok(None);
    };

    let mut stmt = conn
        .prepare(
            "SELECT ea.entity_id, ea.id, ea.kind, ea.status,
                    COALESCE(
                        (SELECT value FROM entity_asset_metadata
                         WHERE asset_id = ea.id AND key = 'preview_role' LIMIT 1),
                        'source_thumbnail'
                    ) AS preview_role
             FROM source_ref sr
             JOIN entity_asset ea ON ea.entity_id = sr.entity_id
             WHERE sr.external_id = ?1 AND sr.kind = 'thumbnail'
               AND ea.kind = 'thumbnail' AND ea.status = 'present'
             ORDER BY ea.size DESC, ea.updated_at DESC",
        )
        .map_err(db_err)?;

    let rows = stmt
        .query_map([external_id], |row| {
            Ok(EntityPreview {
                entity_id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
                asset_id: Uuid::parse_str(&row.get::<_, String>(1)?).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
                kind: row.get(2)?,
                status: row.get(3)?,
                preview_role: row.get(4)?,
            })
        })
        .map_err(db_err)?;

    for row in rows {
        let preview = row.map_err(db_err)?;
        if let Some(preview) = reconcile_preview(conn, vault_root, preview)? {
            return Ok(Some(preview));
        }
    }

    Ok(None)
}

fn load_metadata_entries(
    conn: &Connection,
    entity_id: Uuid,
) -> Result<Vec<MetadataEntry>, VaultError> {
    let mut stmt = conn
        .prepare("SELECT key, value, provenance FROM metadata WHERE entity_id = ?1")
        .map_err(db_err)?;

    let rows = stmt
        .query_map([entity_id.to_string()], |row| {
            Ok(MetadataEntry {
                key: row.get(0)?,
                value: row.get(1)?,
                provenance: row.get(2)?,
            })
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;

    Ok(rows)
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::db::{insert_entity, upsert_metadata};
    use crate::import::import;
    use crate::tags::add_tag;
    use archiveos_contract::ImportStrategy;
    use crate::assets::UpsertAssetInput;
    use crate::preview::ROLE_PREVIEW_IMAGE_SMALL;
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;

    #[test]
    fn get_entity_returns_tags_and_metadata() {
        let dir = tempdir().unwrap();
        let scan = dir.path().join("scan");
        std::fs::create_dir_all(&scan).unwrap();
        std::fs::write(scan.join("a.jpg"), b"j").unwrap();

        let vault = Vault::init(dir.path().join("vault")).unwrap();
        import(&vault, &scan, None, ImportStrategy::Reference).unwrap();

        let entity_id: Uuid = vault
            .connection()
            .query_row("SELECT id FROM entity LIMIT 1", [], |row| {
                let s: String = row.get(0)?;
                Ok(Uuid::parse_str(&s).unwrap())
            })
            .unwrap();

        add_tag(vault.connection(), entity_id, "pic").unwrap();
        let detail = get_entity(vault.connection(), entity_id).unwrap();
        assert_eq!(detail.tags, vec!["pic"]);
        assert!(detail.metadata.contains_key("path"));
        assert!(!detail.metadata_entries.is_empty());
    }

    #[test]
    fn resolve_preview_uses_generated_when_video_has_no_linked_thumbnail() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(
            vault.connection(),
            entity_id,
            None,
            Some("video/mp4"),
            0,
            "present",
            &now,
            None,
        )
        .unwrap();
        upsert_metadata(
            vault.connection(),
            entity_id,
            "thumbnail_external_id",
            "vid:thumbnail",
            "archiveos",
        )
        .unwrap();

        let generated_id = crate::assets::upsert_asset(
            vault.connection(),
            &UpsertAssetInput {
                entity_id,
                role: "supporting",
                kind: "preview",
                content_hash: Some("gen"),
                mime: Some("image/webp"),
                size: 1,
                ext: Some(".webp"),
                status: "present",
                storage_strategy: "managed",
                path: Some("/tmp/small.webp"),
            },
        )
        .unwrap();
        crate::assets::upsert_asset_metadata(
            vault.connection(),
            generated_id,
            "preview_role",
            ROLE_PREVIEW_IMAGE_SMALL,
            "archiveos",
        )
        .unwrap();

        let preview = resolve_entity_preview(vault.connection(), entity_id)
            .unwrap()
            .expect("preview");
        assert_eq!(preview.preview_role, ROLE_PREVIEW_IMAGE_SMALL);
        assert_eq!(preview.asset_id, generated_id);
    }

    #[test]
    fn resolve_preview_prefers_source_thumbnail_for_video() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let video_id = Uuid::new_v4();
        let thumb_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(
            vault.connection(),
            video_id,
            None,
            Some("video/mp4"),
            0,
            "present",
            &now,
            None,
        )
        .unwrap();
        insert_entity(
            vault.connection(),
            thumb_id,
            None,
            Some("image/webp"),
            1,
            "present",
            &now,
            None,
        )
        .unwrap();
        upsert_metadata(
            vault.connection(),
            video_id,
            "thumbnail_external_id",
            "vid:thumbnail",
            "archiveos",
        )
        .unwrap();
        crate::import::db::insert_source_ref(
            vault.connection(),
            Uuid::new_v4(),
            thumb_id,
            "youtube",
            "thumbnail",
            "vid:thumbnail",
            None,
            "live",
        )
        .unwrap();
        let thumb_asset_id = crate::assets::upsert_asset(
            vault.connection(),
            &UpsertAssetInput {
                entity_id: thumb_id,
                role: "primary",
                kind: "thumbnail",
                content_hash: Some("thumb"),
                mime: Some("image/webp"),
                size: 1,
                ext: Some(".webp"),
                status: "present",
                storage_strategy: "managed",
                path: Some("/tmp/thumb.webp"),
            },
        )
        .unwrap();
        let generated_id = crate::assets::upsert_asset(
            vault.connection(),
            &UpsertAssetInput {
                entity_id: video_id,
                role: "supporting",
                kind: "preview",
                content_hash: Some("gen"),
                mime: Some("image/webp"),
                size: 1,
                ext: Some(".webp"),
                status: "present",
                storage_strategy: "managed",
                path: Some("/tmp/small.webp"),
            },
        )
        .unwrap();
        crate::assets::upsert_asset_metadata(
            vault.connection(),
            generated_id,
            "preview_role",
            ROLE_PREVIEW_IMAGE_SMALL,
            "archiveos",
        )
        .unwrap();

        let preview = resolve_entity_preview(vault.connection(), video_id)
            .unwrap()
            .expect("preview");
        assert_eq!(preview.preview_role, "source_thumbnail");
        assert_eq!(preview.asset_id, thumb_asset_id);
    }

    #[test]
    fn resolve_preview_prefers_largest_thumbnail_asset() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let video_id = Uuid::new_v4();
        let thumb_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(
            vault.connection(),
            video_id,
            None,
            Some("video/mp4"),
            0,
            "present",
            &now,
            None,
        )
        .unwrap();
        insert_entity(
            vault.connection(),
            thumb_id,
            None,
            Some("image/jpeg"),
            1,
            "present",
            &now,
            None,
        )
        .unwrap();
        upsert_metadata(
            vault.connection(),
            video_id,
            "thumbnail_external_id",
            "vid:thumbnail",
            "archiveos",
        )
        .unwrap();
        crate::import::db::insert_source_ref(
            vault.connection(),
            Uuid::new_v4(),
            thumb_id,
            "youtube",
            "thumbnail",
            "vid:thumbnail",
            None,
            "live",
        )
        .unwrap();
        let small_asset_id = crate::assets::upsert_asset(
            vault.connection(),
            &UpsertAssetInput {
                entity_id: thumb_id,
                role: "supporting",
                kind: "thumbnail",
                content_hash: Some("small"),
                mime: Some("image/jpeg"),
                size: 100,
                ext: Some(".jpg"),
                status: "present",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();
        let _large_asset_id = crate::assets::upsert_asset(
            vault.connection(),
            &UpsertAssetInput {
                entity_id: thumb_id,
                role: "supporting",
                kind: "thumbnail",
                content_hash: Some("large"),
                mime: Some("image/jpeg"),
                size: 36_000,
                ext: Some(".jpg"),
                status: "present",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();

        let preview = resolve_entity_preview(vault.connection(), video_id)
            .unwrap()
            .expect("preview");
        assert_ne!(preview.asset_id, small_asset_id);
        assert_eq!(preview.preview_role, "source_thumbnail");
    }

    #[test]
    fn get_entity_metadata_entries_include_provenance() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        crate::import::db::insert_entity(
            vault.connection(),
            entity_id,
            None,
            None,
            0,
            "present",
            &now,
            None,
        )
        .unwrap();
        crate::import::db::upsert_metadata(
            vault.connection(),
            entity_id,
            "description",
            "from yt",
            "yt-dlp",
        )
        .unwrap();
        crate::import::db::upsert_metadata(
            vault.connection(),
            entity_id,
            "title",
            "User Title",
            "user",
        )
        .unwrap();
        crate::import::db::upsert_metadata(
            vault.connection(),
            entity_id,
            "title",
            "YT Title",
            "yt-dlp",
        )
        .unwrap();

        let detail = get_entity(vault.connection(), entity_id).unwrap();
        assert_eq!(detail.metadata.get("title").map(String::as_str), Some("User Title"));
        assert_eq!(detail.metadata_entries.len(), 3);
    }

    #[test]
    fn resolve_preview_handles_entity_with_null_mime() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(
            vault.connection(),
            entity_id,
            None,
            None,
            0,
            "present",
            &now,
            None,
        )
        .unwrap();

        let preview = resolve_entity_preview(vault.connection(), entity_id).unwrap();
        assert!(preview.is_none());
    }
}
