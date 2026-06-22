use archiveos_contract::{EntityPreviewSummary, VaultError};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::entity::{
    resolve_entity_preview, resolve_timeline_manifest, resolve_timeline_sprite,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectionQuery {
    pub collection_type: Option<String>,
    pub min_member_count: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CollectionSummary {
    pub id: Uuid,
    pub collection_type: String,
    pub title: String,
    pub member_count: i32,
    pub cover_preview: Option<EntityPreviewSummary>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CollectionDetail {
    pub id: Uuid,
    pub collection_type: String,
    pub title: String,
    pub member_count: i32,
    pub cover_preview: Option<EntityPreviewSummary>,
    pub members: Vec<CollectionMemberItem>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CollectionMemberItem {
    pub id: Uuid,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub mime: Option<String>,
    pub source: Option<String>,
    pub status: String,
    pub position: i32,
    pub preview: Option<EntityPreviewSummary>,
    pub timeline_sprite: Option<EntityPreviewSummary>,
    pub timeline_manifest: Option<EntityPreviewSummary>,
    pub primary_asset_id: Option<Uuid>,
    pub primary_asset_status: Option<String>,
    pub duration: Option<String>,
    pub channel: Option<String>,
    pub uploader: Option<String>,
    pub webpage_url: Option<String>,
}

pub fn list_collections(
    conn: &Connection,
    query: &CollectionQuery,
) -> Result<Vec<CollectionSummary>, VaultError> {
    let mut filters = vec!["1=1".to_string()];
    let mut bind_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(collection_type) = &query.collection_type {
        filters.push("c.type = ?".into());
        bind_values.push(Box::new(collection_type.clone()));
    }

    let sql = format!(
        "SELECT c.id, c.type, c.title,
                (
                    SELECT COUNT(*) FROM collection_member cm
                    WHERE cm.collection_id = c.id AND cm.status = 'active'
                ) AS member_count
         FROM collection c
         WHERE {}
         ORDER BY c.title COLLATE NOCASE ASC",
        filters.join(" AND "),
    );

    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(bind_values.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
            ))
        })
        .map_err(db_err)?;

    let min_count = query.min_member_count.unwrap_or(0);
    let mut items = Vec::new();
    for row in rows {
        let (id, collection_type, title, member_count) = row.map_err(db_err)?;
        if member_count < min_count {
            continue;
        }
        let id = Uuid::parse_str(&id).map_err(|e| VaultError::Database {
            detail: e.to_string(),
        })?;
        let cover_preview = if member_count > 0 {
            resolve_collection_cover(conn, id)?
        } else {
            None
        };
        items.push(CollectionSummary {
            id,
            collection_type,
            title,
            member_count,
            cover_preview,
        });
    }

    Ok(items)
}

pub fn get_collection(
    conn: &Connection,
    collection_id: Uuid,
) -> Result<CollectionDetail, VaultError> {
    let row: Option<(String, String, i32)> = conn
        .query_row(
            "SELECT c.type, c.title,
                    (
                        SELECT COUNT(*) FROM collection_member cm
                        WHERE cm.collection_id = c.id AND cm.status = 'active'
                    ) AS member_count
             FROM collection c
             WHERE c.id = ?1",
            [collection_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(db_err)?;

    let Some((collection_type, title, member_count)) = row else {
        return Err(VaultError::NotFound);
    };

    let members = list_collection_members(conn, collection_id)?;
    let cover_preview = if member_count > 0 {
        members
            .first()
            .and_then(|member| member.preview.clone())
            .or_else(|| resolve_collection_cover(conn, collection_id).ok().flatten())
    } else {
        None
    };

    Ok(CollectionDetail {
        id: collection_id,
        collection_type,
        title,
        member_count,
        cover_preview,
        members,
    })
}

pub fn list_collection_members(
    conn: &Connection,
    collection_id: Uuid,
) -> Result<Vec<CollectionMemberItem>, VaultError> {
    let title_order = crate::metadata::title_precedence_sql();
    let sql = format!(
        "SELECT e.id, e.mime, e.status, cm.position,
                (
                    SELECT m.value FROM metadata m
                    WHERE m.entity_id = e.id AND m.key = 'title'
                    ORDER BY {title_order}
                    LIMIT 1
                ) AS title,
                (
                    SELECT sr.source FROM source_ref sr
                    WHERE sr.entity_id = e.id AND sr.kind = 'video'
                    LIMIT 1
                ) AS source,
                (
                    SELECT ea.id FROM entity_asset ea
                    WHERE ea.entity_id = e.id AND ea.role = 'primary'
                    ORDER BY ea.created_at ASC
                    LIMIT 1
                ) AS primary_asset_id,
                (
                    SELECT ea.status FROM entity_asset ea
                    WHERE ea.entity_id = e.id AND ea.role = 'primary'
                    ORDER BY ea.created_at ASC
                    LIMIT 1
                ) AS primary_asset_status,
                (
                    SELECT ea.kind FROM entity_asset ea
                    WHERE ea.entity_id = e.id AND ea.role = 'primary'
                    ORDER BY ea.created_at ASC
                    LIMIT 1
                ) AS primary_kind
         FROM collection_member cm
         JOIN entity e ON e.id = cm.entity_id
         WHERE cm.collection_id = ?1 AND cm.status = 'active'
         ORDER BY cm.position ASC, e.added_at DESC",
    );

    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let rows = stmt
        .query_map([collection_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
            ))
        })
        .map_err(db_err)?;

    let mut items = Vec::new();
    for row in rows {
        let (
            id,
            mime,
            status,
            position,
            title,
            source,
            primary_asset_id,
            primary_asset_status,
            kind,
        ) = row.map_err(db_err)?;
        let entity_id = Uuid::parse_str(&id).map_err(|e| VaultError::Database {
            detail: e.to_string(),
        })?;
        let primary_asset_id = primary_asset_id
            .map(|value| Uuid::parse_str(&value))
            .transpose()
            .map_err(|e| VaultError::Database {
                detail: e.to_string(),
            })?;

        items.push(CollectionMemberItem {
            id: entity_id,
            title,
            kind,
            mime,
            source,
            status,
            position,
            preview: resolve_entity_preview(conn, entity_id)?.map(EntityPreviewSummary::from),
            timeline_sprite: resolve_timeline_sprite(conn, entity_id)?
                .map(EntityPreviewSummary::from),
            timeline_manifest: resolve_timeline_manifest(conn, entity_id)?
                .map(EntityPreviewSummary::from),
            primary_asset_id,
            primary_asset_status,
            duration: metadata_value(conn, entity_id, "duration")?,
            channel: metadata_value(conn, entity_id, "channel")?,
            uploader: metadata_value(conn, entity_id, "uploader")?,
            webpage_url: metadata_value(conn, entity_id, "webpage_url")?,
        });
    }

    Ok(items)
}

fn resolve_collection_cover(
    conn: &Connection,
    collection_id: Uuid,
) -> Result<Option<EntityPreviewSummary>, VaultError> {
    let entity_id: Option<String> = conn
        .query_row(
            "SELECT cm.entity_id FROM collection_member cm
             WHERE cm.collection_id = ?1 AND cm.status = 'active'
             ORDER BY cm.position ASC
             LIMIT 1",
            [collection_id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;

    let Some(id) = entity_id else {
        return Ok(None);
    };
    let entity_id = Uuid::parse_str(&id).map_err(|e| VaultError::Database {
        detail: e.to_string(),
    })?;
    Ok(resolve_entity_preview(conn, entity_id)?.map(EntityPreviewSummary::from))
}

fn metadata_value(
    conn: &Connection,
    entity_id: Uuid,
    key: &str,
) -> Result<Option<String>, VaultError> {
    let title_order = crate::metadata::title_precedence_sql();
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
    use crate::import::db::{insert_collection, insert_entity, link_collection_member_direct};
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn temp_vault() -> Vault {
        let dir = tempdir().unwrap();
        Vault::init(dir.path()).unwrap()
    }

    fn setup_collection_with_members(member_count: i32) -> (Vault, Uuid) {
        let vault = temp_vault();
        let conn = vault.connection();
        let collection_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(conn, collection_id, None, None, 0, "active", &now, None).unwrap();
        insert_collection(conn, collection_id, "youtube_playlist", "Test Playlist").unwrap();

        for i in 0..member_count {
            let entity_id = Uuid::new_v4();
            insert_entity(conn, entity_id, None, None, 0, "present", &now, None).unwrap();
            link_collection_member_direct(conn, collection_id, entity_id, i).unwrap();
        }

        (vault, collection_id)
    }

    #[test]
    fn list_collections_filters_by_type_and_min_member_count() {
        let vault = temp_vault();
        let conn = vault.connection();
        let now = Utc::now().to_rfc3339();

        let empty_playlist = Uuid::new_v4();
        insert_entity(conn, empty_playlist, None, None, 0, "active", &now, None).unwrap();
        insert_collection(conn, empty_playlist, "youtube_playlist", "Empty").unwrap();

        let (_, populated) = setup_collection_with_members(2);

        let all = list_collections(conn, &CollectionQuery::default()).unwrap();
        assert!(all.iter().any(|c| c.id == populated));
        assert!(all.iter().any(|c| c.id == empty_playlist));

        let youtube_nonempty = list_collections(
            conn,
            &CollectionQuery {
                collection_type: Some("youtube_playlist".into()),
                min_member_count: Some(1),
            },
        )
        .unwrap();
        assert!(youtube_nonempty.iter().any(|c| c.id == populated));
        assert!(!youtube_nonempty.iter().any(|c| c.id == empty_playlist));
    }

    #[test]
    fn get_collection_returns_detail_with_members() {
        let (vault, collection_id) = setup_collection_with_members(3);
        let detail = get_collection(vault.connection(), collection_id).unwrap();
        assert_eq!(detail.collection_type, "youtube_playlist");
        assert_eq!(detail.title, "Test Playlist");
        assert_eq!(detail.member_count, 3);
        assert_eq!(detail.members.len(), 3);
        assert_eq!(detail.members[0].position, 0);
    }

    #[test]
    fn get_collection_missing_returns_not_found() {
        let vault = temp_vault();
        let err = get_collection(vault.connection(), Uuid::new_v4()).unwrap_err();
        assert!(matches!(err, VaultError::NotFound));
    }
}
