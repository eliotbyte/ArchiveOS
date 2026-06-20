use std::collections::HashMap;

use archiveos_contract::VaultError;
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

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
}

pub fn get_entity(conn: &Connection, entity_id: Uuid) -> Result<EntityDetail, VaultError> {
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
    let metadata = load_metadata(conn, id)?;

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
    })
}

fn load_metadata(
    conn: &Connection,
    entity_id: Uuid,
) -> Result<HashMap<String, String>, VaultError> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM metadata WHERE entity_id = ?1")
        .map_err(db_err)?;

    let rows = stmt
        .query_map([entity_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;

    Ok(rows.into_iter().collect())
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
    use crate::tags::add_tag;
    use archiveos_contract::ImportStrategy;
    use crate::Vault;
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
    }
}
