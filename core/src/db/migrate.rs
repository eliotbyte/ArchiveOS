use archiveos_contract::{VaultError, DB_SCHEMA_VERSION};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};

pub fn migrate(conn: &Connection) -> Result<(), VaultError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )
    .map_err(db_err)?;

    let current: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(db_err)?;

    if current >= DB_SCHEMA_VERSION {
        return Ok(());
    }

    if current == 0 {
        conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![1, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 1 {
        conn.execute_batch(include_str!("../../migrations/002_collection_fingerprint.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![2, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 2 {
        conn.execute_batch(include_str!("../../migrations/003_ytdlp_pipeline.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![3, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 3 {
        conn.execute_batch(include_str!("../../migrations/004_entity_roles_provenance.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![4, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 4 {
        conn.execute_batch(include_str!("../../migrations/005_entity_assets.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![5, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 5 {
        conn.execute_batch(include_str!("../../migrations/006_entity_asset_metadata.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![6, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 6 {
        conn.execute_batch(include_str!("../../migrations/007_subscription_options.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![7, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 7 {
        conn.execute_batch(include_str!("../../migrations/008_user_library.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![8, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 8 {
        conn.execute_batch(include_str!("../../migrations/009_user_media_preference.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![9, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 9 {
        conn.execute_batch(include_str!("../../migrations/010_job_progress.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![10, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 10 {
        conn.execute_batch(include_str!("../../migrations/011_youtube_source_canonical.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![11, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    if current <= 11 {
        conn.execute_batch(include_str!("../../migrations/012_youtube_source_repair.sql"))
            .map_err(db_err)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![DB_SCHEMA_VERSION, Utc::now().to_rfc3339()],
        )
        .map_err(db_err)?;
    }

    Ok(())
}

pub fn open_connection(path: &std::path::Path) -> Result<Connection, VaultError> {
    Connection::open(path).map_err(db_err)
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

pub fn table_exists(conn: &Connection, name: &str) -> Result<bool, VaultError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(()),
        )
        .optional()
        .map_err(db_err)?
        .is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use archiveos_contract::DB_SCHEMA_VERSION;

    fn in_memory_conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn migrate_creates_entity_table() {
        let conn = in_memory_conn();
        migrate(&conn).unwrap();
        assert!(table_exists(&conn, "entity").unwrap());
    }

    #[test]
    fn migrate_creates_all_plan_tables() {
        let conn = in_memory_conn();
        migrate(&conn).unwrap();
        for table in [
            "entity",
            "entity_asset",
            "entity_asset_metadata",
            "source_ref",
            "metadata",
            "tag",
            "entity_tag",
            "collection",
            "collection_member",
            "job",
            "source_failure",
            "source_subscription",
            "entity_relation",
            "schema_migrations",
        ] {
            assert!(table_exists(&conn, table).unwrap(), "missing table {table}");
        }
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = in_memory_conn();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();

        let version: i32 = conn
            .query_row(
                "SELECT MAX(version) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, DB_SCHEMA_VERSION);
    }

    #[test]
    fn migrate_v2_adds_content_fingerprint_column() {
        let conn = in_memory_conn();
        migrate(&conn).unwrap();
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('collection') WHERE name = 'content_fingerprint'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migrate_v5_adds_asset_and_membership_lifecycle() {
        let conn = in_memory_conn();
        migrate(&conn).unwrap();

        let asset_columns: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('entity_asset')
                 WHERE name IN ('role', 'kind', 'content_hash', 'status', 'storage_strategy')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(asset_columns, 5);

        let membership_columns: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('collection_member')
                 WHERE name IN ('status', 'first_seen_at', 'last_seen_at', 'removed_at')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(membership_columns, 4);
    }

    #[test]
    fn migrate_v6_adds_entity_asset_metadata_table() {
        let conn = in_memory_conn();
        migrate(&conn).unwrap();
        assert!(table_exists(&conn, "entity_asset_metadata").unwrap());
    }

    #[test]
    fn migrate_v7_adds_subscription_options_json() {
        let conn = in_memory_conn();
        migrate(&conn).unwrap();
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('source_subscription') WHERE name = 'options_json'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migrate_v8_adds_user_library_tables() {
        let conn = in_memory_conn();
        migrate(&conn).unwrap();
        for table in [
            "user_profile",
            "user_playback_state",
            "user_list",
            "user_list_member",
        ] {
            assert!(table_exists(&conn, table).unwrap(), "missing table {table}");
        }
    }

    #[test]
    fn migrate_v9_adds_user_media_preference_table() {
        let conn = in_memory_conn();
        migrate(&conn).unwrap();
        assert!(table_exists(&conn, "user_media_preference").unwrap());
    }
}
