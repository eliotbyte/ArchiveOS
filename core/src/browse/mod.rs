use archiveos_contract::{BrowseQuery, EntityListItem, EntityPreviewSummary, VaultError};
use rusqlite::{params_from_iter, Connection, Row, ToSql};
use uuid::Uuid;

use crate::entity::resolve_entity_preview;
use crate::metadata;
use crate::tags;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

pub fn browse(conn: &Connection, query: &BrowseQuery) -> Result<Vec<EntityListItem>, VaultError> {
    let limit = normalize_limit(query.limit);
    let (sql, bind_values) = build_browse_sql(query, limit);
    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let mut rows = stmt
        .query_map(params_from_iter(bind_values.iter()), map_row)
        .map_err(db_err)?;

    let mut items = Vec::new();
    for row in rows.by_ref() {
        let mut item = row.map_err(db_err)?;
        item.tags = tags::list_entity_tags(conn, item.id)?;
        item.preview = resolve_entity_preview(conn, item.id)?
            .map(EntityPreviewSummary::from);
        items.push(item);
    }

    Ok(items)
}

fn normalize_limit(limit: u32) -> i64 {
    let limit = if limit == 0 { DEFAULT_LIMIT } else { limit };
    i64::from(limit.min(MAX_LIMIT))
}

fn build_browse_sql(query: &BrowseQuery, limit: i64) -> (String, Vec<Box<dyn ToSql>>) {
    let hidden = if query.include_hidden {
        String::new()
    } else {
        format!(" AND {}", metadata::hidden_entity_sql("e"))
    };
    let title_order = metadata::title_precedence_sql();

    let mut filters = vec!["e.status != 'user_deleted'".to_string()];
    let mut bind_values: Vec<Box<dyn ToSql>> = Vec::new();

    if let Some(text) = &query.text {
        filters.push(
            "EXISTS (
                SELECT 1 FROM metadata m
                WHERE m.entity_id = e.id
                  AND m.value LIKE '%' || ? || '%' ESCAPE '\\'
            )"
            .into(),
        );
        bind_values.push(Box::new(text.clone()));
    }
    if let Some(kind) = &query.kind {
        filters.push(
            "EXISTS (
                SELECT 1 FROM entity_asset ea
                WHERE ea.entity_id = e.id AND ea.role = 'primary' AND ea.kind = ?
            )"
            .into(),
        );
        bind_values.push(Box::new(kind.clone()));
    }
    if let Some(source) = &query.source {
        filters.push(
            "EXISTS (
                SELECT 1 FROM source_ref sr
                WHERE sr.entity_id = e.id AND sr.source = ?
            )"
            .into(),
        );
        bind_values.push(Box::new(source.clone()));
    }
    if let Some(status) = &query.status {
        filters.push("e.status = ?".into());
        bind_values.push(Box::new(status.clone()));
    }

    bind_values.push(Box::new(limit));

    let sql = format!(
        "SELECT e.id, e.mime, e.status,
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
         FROM entity e
         WHERE {filters}{hidden}
         ORDER BY e.added_at DESC
         LIMIT ?",
        filters = filters.join(" AND "),
    );

    (sql, bind_values)
}

fn map_row(row: &Row<'_>) -> rusqlite::Result<EntityListItem> {
    let id: String = row.get(0)?;
    let mime: Option<String> = row.get(1)?;
    let status: String = row.get(2)?;
    let title: Option<String> = row.get(3)?;
    let source: Option<String> = row.get(4)?;
    let primary_asset_id: Option<String> = row.get(5)?;
    let primary_asset_status: Option<String> = row.get(6)?;
    let kind: Option<String> = row.get(7)?;

    Ok(EntityListItem {
        id: Uuid::parse_str(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        title,
        mime,
        kind,
        status,
        source,
        tags: Vec::new(),
        preview: None,
        primary_asset_id: primary_asset_id
            .map(|value| Uuid::parse_str(&value))
            .transpose()
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        primary_asset_status,
    })
}

impl From<crate::entity::EntityPreview> for EntityPreviewSummary {
    fn from(preview: crate::entity::EntityPreview) -> Self {
        Self {
            entity_id: preview.entity_id,
            asset_id: preview.asset_id,
            kind: preview.kind,
            preview_role: preview.preview_role,
            status: preview.status,
        }
    }
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
    use crate::tags::add_tag;
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn browse_lists_recent_entities_without_search_params() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        for (title, status) in [("Alpha", "present"), ("Beta", "active")] {
            let entity_id = Uuid::new_v4();
            let now = Utc::now().to_rfc3339();
            insert_entity(conn, entity_id, None, Some("video/mp4"), 0, status, &now, None)
                .unwrap();
            upsert_metadata(conn, entity_id, "title", title, "user").unwrap();
        }

        let items = browse(conn, &BrowseQuery::recent(10)).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.title.as_deref() == Some("Alpha")));
        assert!(items.iter().any(|item| item.title.as_deref() == Some("Beta")));
    }

    #[test]
    fn browse_hides_hidden_entities_by_default() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(conn, entity_id, None, Some("image/webp"), 10, "present", &now, None)
            .unwrap();
        upsert_metadata(conn, entity_id, "title", "Hidden Thumb", "yt-dlp").unwrap();
        upsert_metadata(conn, entity_id, "visibility", "hidden", "archiveos").unwrap();
        add_tag(conn, entity_id, "gallery").unwrap();

        let hidden = browse(conn, &BrowseQuery::recent(10)).unwrap();
        assert!(hidden.is_empty());

        let visible = browse(
            conn,
            &BrowseQuery {
                include_hidden: true,
                limit: 10,
                ..BrowseQuery::default()
            },
        )
        .unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, entity_id);
    }

    #[test]
    fn browse_filters_by_text() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let cat = Uuid::new_v4();
        let dog = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(conn, cat, None, None, 0, "present", &now, None).unwrap();
        insert_entity(conn, dog, None, None, 0, "present", &now, None).unwrap();
        upsert_metadata(conn, cat, "title", "Cute Cat", "user").unwrap();
        upsert_metadata(conn, dog, "title", "Happy Dog", "user").unwrap();

        let hits = browse(
            conn,
            &BrowseQuery {
                text: Some("Cat".into()),
                limit: 10,
                ..BrowseQuery::default()
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, cat);
    }
}
