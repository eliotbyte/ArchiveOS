use archiveos_contract::{EntityHit, SearchQuery, VaultError};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::metadata;

pub fn search(conn: &Connection, query: &SearchQuery) -> Result<Vec<EntityHit>, VaultError> {
    if query.tag.is_none() && query.text.is_none() {
        return Err(VaultError::InvalidLayout {
            detail: "search requires --tag and/or --query".into(),
        });
    }

    let sql = build_search_sql(query);
    let mut stmt = conn.prepare(&sql).map_err(db_err)?;

    let mut hits = Vec::new();
    let rows = match (&query.tag, &query.text) {
        (Some(tag), Some(text)) => stmt.query_map(params![tag, text], map_hit),
        (Some(tag), None) => stmt.query_map(params![tag], map_hit),
        (None, Some(text)) => stmt.query_map(params![text], map_hit),
        (None, None) => unreachable!(),
    }
    .map_err(db_err)?;

    for row in rows {
        hits.push(row.map_err(db_err)?);
    }

    for hit in &mut hits {
        hit.tags = load_tags(conn, hit.id)?;
        if hit.title.is_none() {
            hit.title = load_title(conn, hit.id)?;
        }
    }

    Ok(hits)
}

fn build_search_sql(query: &SearchQuery) -> String {
    let hidden = if query.include_hidden {
        String::new()
    } else {
        format!(" AND {}", metadata::hidden_entity_sql("e"))
    };
    let title_order = metadata::title_precedence_sql();

    match (&query.tag, &query.text) {
        (Some(_), Some(_)) => {
            format!(
                "SELECT DISTINCT e.id, e.content_hash, e.mime, (
                SELECT m.value FROM metadata m
                WHERE m.entity_id = e.id AND m.key = 'title'
                ORDER BY {title_order}
                LIMIT 1
             ) AS title
             FROM entity e
             INNER JOIN entity_tag et ON et.entity_id = e.id
             INNER JOIN tag t ON t.id = et.tag_id
             WHERE t.name = ?1
               AND EXISTS (
                 SELECT 1 FROM metadata m
                 WHERE m.entity_id = e.id
                   AND m.value LIKE '%' || ?2 || '%' ESCAPE '\\'
               ){hidden}
             ORDER BY e.added_at DESC"
            )
        }
        (Some(_), None) => {
            format!(
                "SELECT DISTINCT e.id, e.content_hash, e.mime, (
                SELECT m.value FROM metadata m
                WHERE m.entity_id = e.id AND m.key = 'title'
                ORDER BY {title_order}
                LIMIT 1
             ) AS title
             FROM entity e
             INNER JOIN entity_tag et ON et.entity_id = e.id
             INNER JOIN tag t ON t.id = et.tag_id
             WHERE t.name = ?1{hidden}
             ORDER BY e.added_at DESC"
            )
        }
        (None, Some(_)) => {
            format!(
                "SELECT DISTINCT e.id, e.content_hash, e.mime, (
                SELECT m.value FROM metadata m
                WHERE m.entity_id = e.id AND m.key = 'title'
                ORDER BY {title_order}
                LIMIT 1
             ) AS title
             FROM entity e
             WHERE EXISTS (
                 SELECT 1 FROM metadata m
                 WHERE m.entity_id = e.id
                   AND m.value LIKE '%' || ?1 || '%' ESCAPE '\\'
             ){hidden}
             ORDER BY e.added_at DESC"
            )
        }
        (None, None) => String::new(),
    }
}

fn map_hit(row: &Row<'_>) -> rusqlite::Result<EntityHit> {
    let id: String = row.get(0)?;
    let content_hash: Option<String> = row.get(1)?;
    let mime: Option<String> = row.get(2)?;
    let title: Option<String> = row.get(3)?;

    Ok(EntityHit {
        id: Uuid::parse_str(&id).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        content_hash,
        mime,
        title,
        tags: Vec::new(),
    })
}

fn load_tags(conn: &Connection, entity_id: Uuid) -> Result<Vec<String>, VaultError> {
    crate::tags::list_entity_tags(conn, entity_id)
}

fn load_title(conn: &Connection, entity_id: Uuid) -> Result<Option<String>, VaultError> {
    let title_order = metadata::title_precedence_sql();
    conn.query_row(
        &format!(
            "SELECT value FROM metadata
         WHERE entity_id = ?1 AND key = 'title'
         ORDER BY {title_order}
         LIMIT 1"
        ),
        [entity_id.to_string()],
        |row| row.get(0),
    )
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
    use crate::import::import;
    use crate::import::db::{insert_entity, upsert_metadata};
    use crate::tags::add_tag;
    use archiveos_contract::ImportStrategy;
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn vault_with_indexed_files() -> (tempfile::TempDir, Vault, Uuid, Uuid) {
        let dir = tempdir().unwrap();
        let scan = dir.path().join("scan");
        std::fs::create_dir_all(&scan).unwrap();
        std::fs::write(scan.join("cats.jpg"), b"cat").unwrap();
        std::fs::write(scan.join("dogs.jpg"), b"dog").unwrap();

        let vault = Vault::init(dir.path().join("vault")).unwrap();
        import(&vault, &scan, None, ImportStrategy::Reference).unwrap();

        let conn = vault.connection();
        conn.execute(
            "INSERT INTO metadata (entity_id, key, value, provenance)
             SELECT id, 'title', 'Cute Cat', 'user' FROM entity WHERE id IN (
               SELECT id FROM entity ORDER BY added_at LIMIT 1
             )",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO metadata (entity_id, key, value, provenance)
             SELECT id, 'title', 'Happy Dog', 'user' FROM entity WHERE id IN (
               SELECT id FROM entity ORDER BY added_at DESC LIMIT 1
             )",
            [],
        )
        .unwrap();

        let mut ids = conn
            .prepare("SELECT id FROM entity ORDER BY added_at")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|r| Uuid::parse_str(&r.unwrap()).unwrap())
            .collect::<Vec<_>>();

        let dog = ids.pop().unwrap();
        let cat = ids.pop().unwrap();
        (dir, vault, cat, dog)
    }

    #[test]
    fn search_by_tag() {
        let (_dir, vault, cat, dog) = vault_with_indexed_files();
        let conn = vault.connection();

        add_tag(conn, cat, "animals").unwrap();
        add_tag(conn, dog, "animals").unwrap();
        add_tag(conn, cat, "meme").unwrap();

        let hits = search(conn, &SearchQuery::tag("meme")).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, cat);
        assert!(hits[0].tags.contains(&"meme".to_string()));
    }

    #[test]
    fn search_by_text_in_metadata() {
        let (_dir, vault, cat, _dog) = vault_with_indexed_files();
        let conn = vault.connection();

        let hits = search(conn, &SearchQuery::text("Cat")).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, cat);
        assert_eq!(hits[0].title.as_deref(), Some("Cute Cat"));
    }

    #[test]
    fn search_by_tag_and_text() {
        let (_dir, vault, cat, dog) = vault_with_indexed_files();
        let conn = vault.connection();

        add_tag(conn, cat, "animals").unwrap();
        add_tag(conn, dog, "animals").unwrap();

        let hits = search(
            conn,
            &SearchQuery {
                tag: Some("animals".into()),
                text: Some("Dog".into()),
                include_hidden: false,
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, dog);
    }

    #[test]
    fn search_hides_hidden_entities_by_default() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path().join("vault")).unwrap();
        let conn = vault.connection();
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(conn, entity_id, Some("abc"), Some("image/webp"), 10, "present", &now, None)
            .unwrap();
        upsert_metadata(conn, entity_id, "title", "Hidden Thumb", "yt-dlp").unwrap();
        upsert_metadata(conn, entity_id, "visibility", "hidden", "archiveos").unwrap();
        add_tag(conn, entity_id, "gallery").unwrap();

        let hidden = search(
            conn,
            &SearchQuery {
                tag: Some("gallery".into()),
                text: None,
                include_hidden: false,
            },
        )
        .unwrap();
        assert!(hidden.is_empty());

        let visible = search(
            conn,
            &SearchQuery {
                tag: Some("gallery".into()),
                text: None,
                include_hidden: true,
            },
        )
        .unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, entity_id);
    }
}
