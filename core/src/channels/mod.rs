use std::collections::HashMap;

use archiveos_contract::{ChannelDetail, EntityListItem, EntityPreviewSummary, VaultError};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use uuid::Uuid;

use crate::assets::find_asset_by_preview_role;
use crate::entity::EntityPreview;
use crate::metadata;

#[derive(Debug, Clone)]
pub struct UploadedByChannel {
    pub channel_entity_id: Uuid,
    pub title: Option<String>,
    pub avatar_preview: Option<EntityPreview>,
}

pub fn resolve_channel_avatar_preview(
    conn: &Connection,
    channel_entity_id: Uuid,
) -> Result<Option<EntityPreview>, VaultError> {
    let Some(asset) = find_asset_by_preview_role(conn, channel_entity_id, "avatar")? else {
        return Ok(None);
    };
    if asset.status != "present" {
        return Ok(None);
    }
    Ok(Some(EntityPreview {
        entity_id: channel_entity_id,
        asset_id: asset.id,
        kind: asset.kind,
        preview_role: "avatar".to_string(),
        status: asset.status,
    }))
}

pub fn resolve_uploaded_by_channel(
    conn: &Connection,
    video_entity_id: Uuid,
) -> Result<Option<UploadedByChannel>, VaultError> {
    if let Some(channel) = resolve_uploaded_by_relation(conn, video_entity_id)? {
        return Ok(Some(channel));
    }
    resolve_uploaded_by_metadata(conn, video_entity_id, true)
}

pub(crate) fn resolve_uploaded_by_relation(
    conn: &Connection,
    video_entity_id: Uuid,
) -> Result<Option<UploadedByChannel>, VaultError> {
    let channel_entity_id: Option<String> = conn
        .query_row(
            "SELECT to_entity_id FROM entity_relation
             WHERE from_entity_id = ?1 AND relation = 'uploaded_by'
             LIMIT 1",
            [video_entity_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;

    let Some(channel_entity_id) = channel_entity_id else {
        return Ok(None);
    };
    let channel_entity_id = Uuid::parse_str(&channel_entity_id).map_err(|err| {
        VaultError::Database {
            detail: err.to_string(),
        }
    })?;

    Ok(Some(UploadedByChannel {
        channel_entity_id,
        title: metadata_value(conn, channel_entity_id, "title")?,
        avatar_preview: resolve_channel_avatar_preview(conn, channel_entity_id)?,
    }))
}

fn video_platform_source(conn: &Connection, video_entity_id: Uuid) -> Result<String, VaultError> {
    let source: Option<String> = conn
        .query_row(
            "SELECT source FROM source_ref WHERE entity_id = ?1 AND kind = 'video' LIMIT 1",
            [video_entity_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;
    Ok(match source {
        Some(source) => crate::import::db::canonical_platform_source(&source).to_string(),
        None => "youtube".to_string(),
    })
}

fn resolve_uploaded_by_metadata(
    conn: &Connection,
    video_entity_id: Uuid,
    link_relation: bool,
) -> Result<Option<UploadedByChannel>, VaultError> {
    let platform = video_platform_source(conn, video_entity_id)?;
    for key in ["channel_id", "uploader_id"] {
        let Some(external_id) = metadata_value(conn, video_entity_id, key)? else {
            continue;
        };
        let Some(channel_entity_id) =
            crate::import::db::find_entity_by_source_ref(conn, &platform, "channel", &external_id)?
        else {
            continue;
        };
        if link_relation {
            crate::import::db::link_entity_relation(
                conn,
                video_entity_id,
                channel_entity_id,
                "uploaded_by",
            )?;
        }
        return Ok(Some(UploadedByChannel {
            channel_entity_id,
            title: metadata_value(conn, channel_entity_id, "title")?,
            avatar_preview: resolve_channel_avatar_preview(conn, channel_entity_id)?,
        }));
    }
    Ok(None)
}

pub fn batch_resolve_uploaded_by_channels(
    conn: &Connection,
    video_entity_ids: &[Uuid],
) -> Result<HashMap<Uuid, UploadedByChannel>, VaultError> {
    if video_entity_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders: Vec<String> = video_entity_ids.iter().map(|_| "?".into()).collect();
    let sql = format!(
        "SELECT from_entity_id, to_entity_id FROM entity_relation
         WHERE relation = 'uploaded_by' AND from_entity_id IN ({})",
        placeholders.join(", ")
    );
    let bind_values: Vec<String> = video_entity_ids.iter().map(|id| id.to_string()).collect();

    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let rows = stmt
        .query_map(params_from_iter(bind_values.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(db_err)?;

    let mut video_to_channel = HashMap::new();
    let mut channel_ids = Vec::new();
    for row in rows {
        let (from_id, to_id) = row.map_err(db_err)?;
        let from_id = Uuid::parse_str(&from_id).map_err(|err| VaultError::Database {
            detail: err.to_string(),
        })?;
        let to_id = Uuid::parse_str(&to_id).map_err(|err| VaultError::Database {
            detail: err.to_string(),
        })?;
        video_to_channel.insert(from_id, to_id);
        if !channel_ids.contains(&to_id) {
            channel_ids.push(to_id);
        }
    }

    let mut channel_info = HashMap::new();
    for channel_id in channel_ids {
        channel_info.insert(
            channel_id,
            UploadedByChannel {
                channel_entity_id: channel_id,
                title: metadata_value(conn, channel_id, "title")?,
                avatar_preview: resolve_channel_avatar_preview(conn, channel_id)?,
            },
        );
    }

    let mut result = HashMap::new();
    for (video_id, channel_id) in video_to_channel {
        if let Some(info) = channel_info.get(&channel_id) {
            result.insert(video_id, info.clone());
        }
    }
    for video_id in video_entity_ids {
        if result.contains_key(video_id) {
            continue;
        }
        if let Some(channel) = resolve_uploaded_by_metadata(conn, *video_id, true)? {
            result.insert(*video_id, channel);
        }
    }
    Ok(result)
}

pub fn enrich_entity_list_items(
    conn: &Connection,
    items: &mut [EntityListItem],
) -> Result<(), VaultError> {
    let ids: Vec<Uuid> = items.iter().map(|item| item.id).collect();
    let channels = batch_resolve_uploaded_by_channels(conn, &ids)?;
    for item in items {
        if let Some(channel) = channels.get(&item.id) {
            item.channel_entity_id = Some(channel.channel_entity_id);
            item.channel_avatar_preview = channel
                .avatar_preview
                .clone()
                .map(EntityPreviewSummary::from);
            if item.channel.is_none() {
                item.channel = channel.title.clone();
            }
        }
    }
    Ok(())
}

pub fn get_channel_detail(
    conn: &Connection,
    channel_entity_id: Uuid,
) -> Result<ChannelDetail, VaultError> {
    let is_channel: bool = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM source_ref
                WHERE entity_id = ?1 AND kind = 'channel'
            )",
            [channel_entity_id.to_string()],
            |row| row.get(0),
        )
        .map_err(db_err)?;
    if !is_channel {
        return Err(VaultError::NotFound);
    }

    let source: Option<String> = conn
        .query_row(
            "SELECT source FROM source_ref WHERE entity_id = ?1 LIMIT 1",
            [channel_entity_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;
    let url: Option<String> = conn
        .query_row(
            "SELECT url FROM source_ref WHERE entity_id = ?1 LIMIT 1",
            [channel_entity_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;

    let verified = metadata_value(conn, channel_entity_id, "verified")?
        .map(|value| value == "true")
        .unwrap_or(false);

    Ok(ChannelDetail {
        id: channel_entity_id,
        title: metadata_value(conn, channel_entity_id, "title")?,
        description: metadata_value(conn, channel_entity_id, "description")?,
        follower_count: metadata_value(conn, channel_entity_id, "follower_count")?,
        verified: Some(verified),
        source,
        url,
        avatar_preview: resolve_channel_avatar_preview(conn, channel_entity_id)?
            .map(EntityPreviewSummary::from),
    })
}

fn metadata_value(
    conn: &Connection,
    entity_id: Uuid,
    key: &str,
) -> Result<Option<String>, VaultError> {
    let title_order = metadata::title_precedence_sql();
    let sql = format!(
        "SELECT value FROM metadata
         WHERE entity_id = ?1 AND key = ?2
         ORDER BY {title_order}
         LIMIT 1",
    );
    conn.query_row(&sql, params![entity_id.to_string(), key], |row| row.get(0))
        .optional()
        .map_err(db_err)
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::UpsertAssetInput;
    use crate::import::db::{insert_entity, insert_source_ref, link_entity_relation, upsert_metadata};
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;

    #[test]
    fn resolve_uploaded_by_channel_includes_avatar_preview() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let conn = vault.connection();
        let now = Utc::now().to_rfc3339();

        let video_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();
        insert_entity(conn, video_id, None, Some("video/mp4"), 0, "present", &now, None).unwrap();
        insert_entity(conn, channel_id, None, None, 0, "active", &now, None).unwrap();
        insert_source_ref(
            conn,
            Uuid::new_v4(),
            channel_id,
            "youtube",
            "channel",
            "UC999",
            Some("https://youtube.com/channel/UC999"),
            "live",
        )
        .unwrap();
        upsert_metadata(conn, channel_id, "title", "Avatar Channel", "yt-dlp").unwrap();
        link_entity_relation(conn, video_id, channel_id, "uploaded_by").unwrap();

        let asset_id = crate::assets::upsert_asset(
            conn,
            &UpsertAssetInput {
                entity_id: channel_id,
                role: "supporting",
                kind: "avatar",
                content_hash: Some("abc123"),
                mime: Some("image/jpeg"),
                size: 100,
                ext: Some("jpg"),
                status: "present",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();
        crate::assets::upsert_asset_metadata(conn, asset_id, "preview_role", "avatar", "archiveos")
            .unwrap();

        let resolved = resolve_uploaded_by_channel(conn, video_id).unwrap().unwrap();
        assert_eq!(resolved.channel_entity_id, channel_id);
        assert_eq!(resolved.title.as_deref(), Some("Avatar Channel"));
        assert!(resolved.avatar_preview.is_some());
    }

    #[test]
    fn resolve_uploaded_by_channel_omits_missing_local_avatar() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let conn = vault.connection();
        let now = Utc::now().to_rfc3339();

        let video_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();
        insert_entity(conn, video_id, None, Some("video/mp4"), 0, "present", &now, None).unwrap();
        insert_entity(conn, channel_id, None, None, 0, "active", &now, None).unwrap();
        insert_source_ref(
            conn,
            Uuid::new_v4(),
            channel_id,
            "youtube",
            "channel",
            "UC888",
            Some("https://youtube.com/channel/UC888"),
            "live",
        )
        .unwrap();
        link_entity_relation(conn, video_id, channel_id, "uploaded_by").unwrap();

        let asset_id = crate::assets::upsert_asset(
            conn,
            &UpsertAssetInput {
                entity_id: channel_id,
                role: "supporting",
                kind: "avatar",
                content_hash: Some("deadbeef"),
                mime: Some("image/jpeg"),
                size: 100,
                ext: Some("jpg"),
                status: "missing_local",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();
        crate::assets::upsert_asset_metadata(conn, asset_id, "preview_role", "avatar", "archiveos")
            .unwrap();

        let resolved = resolve_uploaded_by_channel(conn, video_id).unwrap().unwrap();
        assert!(resolved.avatar_preview.is_none());
    }

    #[test]
    fn resolve_uploaded_by_channel_falls_back_to_metadata_channel_id() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let conn = vault.connection();
        let now = Utc::now().to_rfc3339();

        let video_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();
        insert_entity(conn, video_id, None, Some("video/mp4"), 0, "present", &now, None).unwrap();
        insert_entity(conn, channel_id, None, None, 0, "active", &now, None).unwrap();
        insert_source_ref(
            conn,
            Uuid::new_v4(),
            video_id,
            "youtube",
            "video",
            "new-vid",
            Some("https://youtube.com/watch?v=new-vid"),
            "live",
        )
        .unwrap();
        insert_source_ref(
            conn,
            Uuid::new_v4(),
            channel_id,
            "youtube",
            "channel",
            "UC999",
            Some("https://youtube.com/channel/UC999"),
            "live",
        )
        .unwrap();
        upsert_metadata(conn, channel_id, "title", "Avatar Channel", "yt-dlp").unwrap();
        upsert_metadata(conn, video_id, "channel_id", "UC999", "yt-dlp").unwrap();
        upsert_metadata(conn, video_id, "channel", "Avatar Channel", "yt-dlp").unwrap();

        let asset_id = crate::assets::upsert_asset(
            conn,
            &UpsertAssetInput {
                entity_id: channel_id,
                role: "supporting",
                kind: "avatar",
                content_hash: Some("abc123"),
                mime: Some("image/jpeg"),
                size: 100,
                ext: Some("jpg"),
                status: "present",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();
        crate::assets::upsert_asset_metadata(conn, asset_id, "preview_role", "avatar", "archiveos")
            .unwrap();

        let resolved = resolve_uploaded_by_channel(conn, video_id).unwrap().unwrap();
        assert_eq!(resolved.channel_entity_id, channel_id);
        assert!(resolved.avatar_preview.is_some());

        let relation_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM entity_relation WHERE relation = 'uploaded_by'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(relation_count, 1);
    }
}
