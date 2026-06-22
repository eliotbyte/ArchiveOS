use archiveos_contract::VaultError;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceHasHit {
    pub external_id: String,
    pub entity_id: String,
    pub present: bool,
    pub known: bool,
    pub entity_status: String,
    pub source_status: String,
    pub has_present_asset: bool,
    pub asset_statuses: Vec<String>,
}

pub fn sources_has(
    conn: &Connection,
    source: &str,
    kind: &str,
    external_ids: &[String],
) -> Result<Vec<SourceHasHit>, VaultError> {
    let mut hits = Vec::new();
    for external_id in external_ids {
        let row: Option<(String, String, String, String)> = conn
            .query_row(
                "SELECT sr.external_id, sr.entity_id, e.status, sr.status
                 FROM source_ref sr
                 INNER JOIN entity e ON e.id = sr.entity_id
                 WHERE sr.source = ?1 AND sr.kind = ?2 AND sr.external_id = ?3",
                params![source, kind, external_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(db_err)?;

        if let Some((ext_id, entity_id, entity_status, source_status)) = row {
            let entity_uuid = uuid::Uuid::parse_str(&entity_id).map_err(|err| {
                VaultError::InvalidLayout {
                    detail: err.to_string(),
                }
            })?;
            let has_present_asset = crate::assets::has_present_asset(conn, entity_uuid)?;
            let asset_statuses = crate::assets::asset_statuses(conn, entity_uuid)?;
            let present = entity_status != "user_deleted"
                && !matches!(source_status.as_str(), "dead" | "private" | "region_locked")
                && has_present_asset;
            hits.push(SourceHasHit {
                external_id: ext_id,
                entity_id,
                present,
                known: true,
                entity_status,
                source_status,
                has_present_asset,
                asset_statuses,
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
    use crate::assets::{upsert_asset, UpsertAssetInput};
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
        upsert_asset(
            conn,
            &UpsertAssetInput {
                entity_id,
                role: "primary",
                kind: "video",
                content_hash: Some("abc"),
                mime: Some("video/mp4"),
                size: 100,
                ext: Some(".mp4"),
                status: "present",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();

        let hits = sources_has(conn, "youtube", "video", &["vid1".into(), "missing".into()]).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].present);
        assert!(hits[0].known);
        assert_eq!(hits[0].external_id, "vid1");
    }
}
