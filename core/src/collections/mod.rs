use archiveos_contract::{EntityPreviewSummary, VaultError};
use rusqlite::Connection;
use uuid::Uuid;

use crate::entity::resolve_entity_preview;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CollectionSummary {
    pub id: Uuid,
    pub collection_type: String,
    pub title: String,
    pub member_count: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CollectionMemberItem {
    pub id: Uuid,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub status: String,
    pub position: i32,
    pub preview: Option<EntityPreviewSummary>,
}

pub fn list_collections(conn: &Connection) -> Result<Vec<CollectionSummary>, VaultError> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.type, c.title,
                    (
                        SELECT COUNT(*) FROM collection_member cm
                        WHERE cm.collection_id = c.id AND cm.status = 'active'
                    ) AS member_count
             FROM collection c
             ORDER BY c.title COLLATE NOCASE ASC",
        )
        .map_err(db_err)?;

    let rows = stmt
        .query_map([], |row| {
            Ok(CollectionSummary {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
                collection_type: row.get(1)?,
                title: row.get(2)?,
                member_count: row.get(3)?,
            })
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;

    Ok(rows)
}

pub fn list_collection_members(
    conn: &Connection,
    collection_id: Uuid,
) -> Result<Vec<CollectionMemberItem>, VaultError> {
    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.status, cm.position,
                    (
                        SELECT m.value FROM metadata m
                        WHERE m.entity_id = e.id AND m.key = 'title'
                        ORDER BY CASE m.provenance
                            WHEN 'user' THEN 0
                            WHEN 'yt-dlp' THEN 1
                            WHEN 'ffprobe' THEN 2
                            WHEN 'archiveos' THEN 3
                            WHEN 'inferred' THEN 4
                            WHEN 'system' THEN 5
                            WHEN 'source' THEN 6
                            WHEN 'extracted' THEN 7
                            ELSE 8 END
                        LIMIT 1
                    ) AS title,
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
        )
        .map_err(db_err)?;

    let rows = stmt
        .query_map([collection_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(db_err)?;

    let mut items = Vec::new();
    for row in rows {
        let (id, status, position, title, kind) = row.map_err(db_err)?;
        let entity_id = Uuid::parse_str(&id).map_err(|e| VaultError::Database {
            detail: e.to_string(),
        })?;
        items.push(CollectionMemberItem {
            id: entity_id,
            title,
            kind,
            status,
            position,
            preview: resolve_entity_preview(conn, entity_id)?.map(EntityPreviewSummary::from),
        });
    }

    Ok(items)
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}
