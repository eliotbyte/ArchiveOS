use std::collections::HashSet;

use archiveos_contract::VaultError;
use rusqlite::Connection;

pub fn blob_reference_count(conn: &Connection, hash: &str) -> Result<u32, VaultError> {
    let asset_refs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entity_asset
             WHERE content_hash = ?1
               AND storage_strategy = 'managed'
               AND status = 'present'",
            [hash],
            |row| row.get(0),
        )
        .map_err(db_err)?;

    let entity_refs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entity
             WHERE content_hash = ?1 AND status = 'active'",
            [hash],
            |row| row.get(0),
        )
        .map_err(db_err)?;

    Ok(u32::try_from(asset_refs + entity_refs).unwrap_or(u32::MAX))
}

pub fn collect_pinned_hashes(conn: &Connection) -> Result<HashSet<String>, VaultError> {
    let mut pinned = HashSet::new();

    let mut asset_stmt = conn
        .prepare(
            "SELECT DISTINCT content_hash FROM entity_asset
             WHERE content_hash IS NOT NULL
               AND storage_strategy = 'managed'
               AND status = 'present'",
        )
        .map_err(db_err)?;
    let asset_rows = asset_stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(db_err)?;
    for row in asset_rows {
        pinned.insert(row.map_err(db_err)?);
    }

    let mut entity_stmt = conn
        .prepare(
            "SELECT DISTINCT content_hash FROM entity
             WHERE content_hash IS NOT NULL AND status = 'active'",
        )
        .map_err(db_err)?;
    let entity_rows = entity_stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(db_err)?;
    for row in entity_rows {
        pinned.insert(row.map_err(db_err)?);
    }

    Ok(pinned)
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
    use crate::import::db::insert_entity;
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn blob_reference_count_sums_present_assets_and_active_entities() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let hash = "a".repeat(64);
        let entity_a = Uuid::new_v4();
        let entity_b = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();

        insert_entity(conn, entity_a, Some(&hash), Some("image/jpeg"), 1, "active", &now, None)
            .unwrap();
        insert_entity(conn, entity_b, None, Some("image/jpeg"), 1, "active", &now, None).unwrap();

        upsert_asset(
            conn,
            &UpsertAssetInput {
                entity_id: entity_b,
                role: "supporting",
                kind: "thumbnail",
                content_hash: Some(&hash),
                mime: Some("image/jpeg"),
                size: 1,
                ext: Some(".jpg"),
                status: "present",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();

        assert_eq!(blob_reference_count(conn, &hash).unwrap(), 2);

        crate::assets::mark_entity_assets_deleted(conn, entity_b).unwrap();
        assert_eq!(blob_reference_count(conn, &hash).unwrap(), 1);
    }

    #[test]
    fn collect_pinned_hashes_includes_only_present_managed_assets() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let pinned_hash = "b".repeat(64);
        let missing_hash = "c".repeat(64);
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();

        insert_entity(conn, entity_id, None, Some("image/jpeg"), 1, "active", &now, None).unwrap();
        upsert_asset(
            conn,
            &UpsertAssetInput {
                entity_id,
                role: "supporting",
                kind: "thumbnail",
                content_hash: Some(&pinned_hash),
                mime: Some("image/jpeg"),
                size: 1,
                ext: Some(".jpg"),
                status: "present",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();
        upsert_asset(
            conn,
            &UpsertAssetInput {
                entity_id,
                role: "supporting",
                kind: "thumbnail",
                content_hash: Some(&missing_hash),
                mime: Some("image/jpeg"),
                size: 1,
                ext: Some(".jpg"),
                status: "missing",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();

        let pinned = collect_pinned_hashes(conn).unwrap();
        assert!(pinned.contains(&pinned_hash));
        assert!(!pinned.contains(&missing_hash));
    }
}
