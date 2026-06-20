use archiveos_contract::VaultError;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub fn add_tag(conn: &Connection, entity_id: Uuid, tag_name: &str) -> Result<(), VaultError> {
    let tag_name = normalize_tag_name(tag_name)?;
    ensure_entity_exists(conn, entity_id)?;

    conn.execute(
        "INSERT OR IGNORE INTO tag (name) VALUES (?1)",
        [&tag_name],
    )
    .map_err(db_err)?;

    let tag_id: i64 = conn
        .query_row(
            "SELECT id FROM tag WHERE name = ?1",
            [&tag_name],
            |row| row.get(0),
        )
        .map_err(db_err)?;

    conn.execute(
        "INSERT OR IGNORE INTO entity_tag (entity_id, tag_id) VALUES (?1, ?2)",
        params![entity_id.to_string(), tag_id],
    )
    .map_err(db_err)?;

    Ok(())
}

pub fn remove_tag(conn: &Connection, entity_id: Uuid, tag_name: &str) -> Result<(), VaultError> {
    let tag_name = normalize_tag_name(tag_name)?;
    ensure_entity_exists(conn, entity_id)?;

    let tag_id: Option<i64> = conn
        .query_row("SELECT id FROM tag WHERE name = ?1", [&tag_name], |row| {
            row.get(0)
        })
        .optional()
        .map_err(db_err)?;

    let Some(tag_id) = tag_id else {
        return Ok(());
    };

    conn.execute(
        "DELETE FROM entity_tag WHERE entity_id = ?1 AND tag_id = ?2",
        params![entity_id.to_string(), tag_id],
    )
    .map_err(db_err)?;

    Ok(())
}

pub fn list_entity_tags(conn: &Connection, entity_id: Uuid) -> Result<Vec<String>, VaultError> {
    ensure_entity_exists(conn, entity_id)?;

    let mut stmt = conn
        .prepare(
            "SELECT t.name FROM tag t
             INNER JOIN entity_tag et ON et.tag_id = t.id
             WHERE et.entity_id = ?1
             ORDER BY t.name",
        )
        .map_err(db_err)?;

    let tags = stmt
        .query_map([entity_id.to_string()], |row| row.get(0))
        .map_err(db_err)?
        .collect::<Result<Vec<String>, _>>()
        .map_err(db_err)?;

    Ok(tags)
}

fn normalize_tag_name(name: &str) -> Result<String, VaultError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(VaultError::InvalidLayout {
            detail: "tag name must not be empty".into(),
        });
    }
    Ok(trimmed.to_string())
}

fn ensure_entity_exists(conn: &Connection, entity_id: Uuid) -> Result<(), VaultError> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM entity WHERE id = ?1",
            [entity_id.to_string()],
            |_| Ok(()),
        )
        .optional()
        .map_err(db_err)?
        .is_some();

    if !exists {
        return Err(VaultError::NotFound);
    }
    Ok(())
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
    use archiveos_contract::ImportStrategy;
    use crate::Vault;
    use tempfile::tempdir;

    fn vault_with_entity() -> (tempfile::TempDir, Vault, Uuid) {
        let dir = tempdir().unwrap();
        let scan = dir.path().join("scan");
        std::fs::create_dir_all(&scan).unwrap();
        std::fs::write(scan.join("photo.jpg"), b"jpg").unwrap();

        let vault = Vault::init(dir.path().join("vault")).unwrap();
        import(&vault, &scan, None, ImportStrategy::Reference).unwrap();

        let entity_id: String = vault
            .connection()
            .query_row("SELECT id FROM entity LIMIT 1", [], |row| row.get(0))
            .unwrap();
        (dir, vault, Uuid::parse_str(&entity_id).unwrap())
    }

    #[test]
    fn add_and_list_tag() {
        let (_dir, vault, entity_id) = vault_with_entity();
        let conn = vault.connection();

        add_tag(conn, entity_id, "meme").unwrap();
        add_tag(conn, entity_id, "meme").unwrap(); // idempotent

        let tags = list_entity_tags(conn, entity_id).unwrap();
        assert_eq!(tags, vec!["meme"]);
    }

    #[test]
    fn remove_tag_from_entity() {
        let (_dir, vault, entity_id) = vault_with_entity();
        let conn = vault.connection();

        add_tag(conn, entity_id, "funny").unwrap();
        super::remove_tag(conn, entity_id, "funny").unwrap();
        assert!(list_entity_tags(conn, entity_id).unwrap().is_empty());
    }
}
