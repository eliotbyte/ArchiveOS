use archiveos_contract::VaultError;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceHasHit {
    pub external_id: String,
    pub entity_id: String,
    pub present: bool,
}

pub fn sources_has(
    conn: &Connection,
    source: &str,
    kind: &str,
    external_ids: &[String],
) -> Result<Vec<SourceHasHit>, VaultError> {
    let mut hits = Vec::new();
    for external_id in external_ids {
        let row: Option<(String, String, Option<String>)> = conn
            .query_row(
                "SELECT sr.external_id, sr.entity_id, e.status
                 FROM source_ref sr
                 INNER JOIN entity e ON e.id = sr.entity_id
                 WHERE sr.source = ?1 AND sr.kind = ?2 AND sr.external_id = ?3",
                params![source, kind, external_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(db_err)?;

        if let Some((ext_id, entity_id, status)) = row {
            let present = status.as_deref() == Some("present");
            hits.push(SourceHasHit {
                external_id: ext_id,
                entity_id,
                present,
            });
        }
    }
    Ok(hits)
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::db::{insert_entity, insert_source_ref};
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn sources_has_reports_present_entities() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(
            conn,
            entity_id,
            Some("abc"),
            Some("video/mp4"),
            100,
            "present",
            &now,
            None,
        )
        .unwrap();
        insert_source_ref(
            conn,
            Uuid::new_v4(),
            entity_id,
            "youtube",
            "video",
            "vid1",
            None,
            "live",
        )
        .unwrap();

        let hits = sources_has(conn, "youtube", "video", &["vid1".into(), "missing".into()]).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].present);
        assert_eq!(hits[0].external_id, "vid1");
    }
}
