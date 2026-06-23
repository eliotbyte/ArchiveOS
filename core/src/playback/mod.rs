use archiveos_contract::{PlaybackStateInput, PlaybackStateResponse, VaultError};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub const DEFAULT_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

const COMPLETION_THRESHOLD: f64 = 0.92;
const MIN_POSITION_SECONDS: f64 = 30.0;
const MIN_PROGRESS_FRACTION: f64 = 0.05;

pub fn default_user_id() -> Uuid {
    Uuid::parse_str(DEFAULT_USER_ID).expect("valid default user id")
}

pub fn ensure_default_user(conn: &Connection) -> Result<Uuid, VaultError> {
    let user_id = default_user_id();
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM user_profile WHERE id = ?1",
            [user_id.to_string()],
            |_| Ok(()),
        )
        .optional()
        .map_err(db_err)?
        .is_some();
    if !exists {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO user_profile (id, label, created_at) VALUES (?1, ?2, ?3)",
            params![user_id.to_string(), "default", now],
        )
        .map_err(db_err)?;
    }
    Ok(user_id)
}

pub fn upsert_playback_state(
    conn: &Connection,
    user_id: Uuid,
    entity_id: Uuid,
    input: &PlaybackStateInput,
) -> Result<PlaybackStateResponse, VaultError> {
    let now = Utc::now().to_rfc3339();
    let duration = input.duration_seconds;
    let position = input.position_seconds.max(0.0);

    let completed_at = match duration {
        Some(d) if d > 0.0 && position / d >= COMPLETION_THRESHOLD => Some(now.clone()),
        _ => None,
    };

    conn.execute(
        "INSERT INTO user_playback_state
            (user_id, entity_id, asset_id, position_seconds, duration_seconds, completed_at, updated_at, dismissed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
         ON CONFLICT(user_id, entity_id) DO UPDATE SET
            asset_id = excluded.asset_id,
            position_seconds = excluded.position_seconds,
            duration_seconds = COALESCE(excluded.duration_seconds, user_playback_state.duration_seconds),
            completed_at = COALESCE(excluded.completed_at, user_playback_state.completed_at),
            updated_at = excluded.updated_at,
            dismissed_at = NULL",
        params![
            user_id.to_string(),
            entity_id.to_string(),
            input.asset_id.to_string(),
            position,
            duration,
            completed_at,
            now,
        ],
    )
    .map_err(db_err)?;

    get_playback_state(conn, user_id, entity_id)?
        .ok_or(VaultError::Database {
            detail: "playback state missing after upsert".into(),
        })
}

pub fn get_playback_state(
    conn: &Connection,
    user_id: Uuid,
    entity_id: Uuid,
) -> Result<Option<PlaybackStateResponse>, VaultError> {
    conn.query_row(
        "SELECT asset_id, position_seconds, duration_seconds, completed_at, updated_at
         FROM user_playback_state
         WHERE user_id = ?1 AND entity_id = ?2",
        params![user_id.to_string(), entity_id.to_string()],
        |row| {
            let asset_id: String = row.get(0)?;
            Ok(PlaybackStateResponse {
                entity_id,
                asset_id: Uuid::parse_str(&asset_id).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
                position_seconds: row.get(1)?,
                duration_seconds: row.get(2)?,
                completed_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(db_err)
}

pub fn dismiss_playback_state(
    conn: &Connection,
    user_id: Uuid,
    entity_id: Uuid,
) -> Result<(), VaultError> {
    let now = Utc::now().to_rfc3339();
    let changed = conn
        .execute(
            "UPDATE user_playback_state SET dismissed_at = ?3, updated_at = ?3
             WHERE user_id = ?1 AND entity_id = ?2",
            params![user_id.to_string(), entity_id.to_string(), now],
        )
        .map_err(db_err)?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

pub fn is_continue_watching(row: &PlaybackRow) -> bool {
    if row.completed_at.is_some() || row.dismissed_at.is_some() {
        return false;
    }
    let position = row.position_seconds;
    if position < MIN_POSITION_SECONDS {
        if let Some(duration) = row.duration_seconds {
            if duration > 0.0 && position / duration < MIN_PROGRESS_FRACTION {
                return false;
            }
        } else {
            return false;
        }
    }
    if let Some(duration) = row.duration_seconds {
        if duration > 0.0 && position / duration >= COMPLETION_THRESHOLD {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone)]
pub struct PlaybackRow {
    pub entity_id: Uuid,
    pub asset_id: Uuid,
    pub position_seconds: f64,
    pub duration_seconds: Option<f64>,
    pub completed_at: Option<String>,
    pub dismissed_at: Option<String>,
    pub updated_at: String,
}

pub fn list_playback_for_user(
    conn: &Connection,
    user_id: Uuid,
    continue_only: bool,
    limit: i64,
) -> Result<Vec<PlaybackRow>, VaultError> {
    let sql = if continue_only {
        "SELECT entity_id, asset_id, position_seconds, duration_seconds, completed_at, dismissed_at, updated_at
         FROM user_playback_state
         WHERE user_id = ?1 AND dismissed_at IS NULL AND completed_at IS NULL
         ORDER BY updated_at DESC
         LIMIT ?2"
    } else {
        "SELECT entity_id, asset_id, position_seconds, duration_seconds, completed_at, dismissed_at, updated_at
         FROM user_playback_state
         WHERE user_id = ?1 AND dismissed_at IS NULL
         ORDER BY updated_at DESC
         LIMIT ?2"
    };

    let mut stmt = conn.prepare(sql).map_err(db_err)?;
    let rows = stmt
        .query_map(params![user_id.to_string(), limit], |row| {
            let entity_id: String = row.get(0)?;
            let asset_id: String = row.get(1)?;
            Ok(PlaybackRow {
                entity_id: Uuid::parse_str(&entity_id).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
                asset_id: Uuid::parse_str(&asset_id).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
                position_seconds: row.get(2)?,
                duration_seconds: row.get(3)?,
                completed_at: row.get(4)?,
                dismissed_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(db_err)?;

    let mut items = Vec::new();
    for row in rows {
        let item = row.map_err(db_err)?;
        if continue_only && !is_continue_watching(&item) {
            continue;
        }
        items.push(item);
    }
    Ok(items)
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vault;
    use tempfile::tempdir;

    #[test]
    fn upsert_and_get_playback_state() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let user_id = ensure_default_user(conn).unwrap();
        let entity_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();

        let response = upsert_playback_state(
            conn,
            user_id,
            entity_id,
            &PlaybackStateInput {
                asset_id,
                position_seconds: 120.0,
                duration_seconds: Some(600.0),
            },
        )
        .unwrap();

        assert_eq!(response.position_seconds, 120.0);
        assert!(response.completed_at.is_none());

        let loaded = get_playback_state(conn, user_id, entity_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.position_seconds, 120.0);
    }

    #[test]
    fn marks_completed_near_end() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let user_id = ensure_default_user(conn).unwrap();
        let entity_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();

        let response = upsert_playback_state(
            conn,
            user_id,
            entity_id,
            &PlaybackStateInput {
                asset_id,
                position_seconds: 950.0,
                duration_seconds: Some(1000.0),
            },
        )
        .unwrap();

        assert!(response.completed_at.is_some());
    }
}
