use archiveos_contract::{UserMediaPreferences, VaultError};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::playback::ensure_default_user;

const PREF_KEY: &str = "media";

pub fn get_media_preferences(conn: &Connection) -> Result<UserMediaPreferences, VaultError> {
    let user_id = ensure_default_user(conn)?;
    get_media_preferences_for_user(conn, user_id)
}

pub fn get_media_preferences_for_user(
    conn: &Connection,
    user_id: Uuid,
) -> Result<UserMediaPreferences, VaultError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value_json FROM user_media_preference WHERE user_id = ?1 AND key = ?2",
            params![user_id.to_string(), PREF_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;

    match raw {
        Some(json) => serde_json::from_str(&json).map_err(VaultError::from),
        None => Ok(UserMediaPreferences::default()),
    }
}

pub fn set_media_preferences(
    conn: &Connection,
    prefs: &UserMediaPreferences,
) -> Result<UserMediaPreferences, VaultError> {
    let user_id = ensure_default_user(conn)?;
    let now = Utc::now().to_rfc3339();
    let json = serde_json::to_string(prefs).map_err(VaultError::from)?;
    conn.execute(
        "INSERT INTO user_media_preference (user_id, key, value_json, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id, key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at",
        params![user_id.to_string(), PREF_KEY, json, now],
    )
    .map_err(db_err)?;
    get_media_preferences(conn)
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
    fn defaults_when_missing() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let prefs = get_media_preferences(vault.connection()).unwrap();
        assert_eq!(prefs.subtitle_mode, "preferred");
        assert_eq!(prefs.subtitle_languages, vec!["ru", "en", "original"]);
    }

    #[test]
    fn roundtrip_preferences() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let input = UserMediaPreferences {
            subtitle_languages: vec!["original".into(), "de".into()],
            subtitle_mode: "all".into(),
            ..UserMediaPreferences::default()
        };
        let saved = set_media_preferences(conn, &input).unwrap();
        assert_eq!(saved, input);
    }

    #[test]
    fn to_asset_policy_maps_video_quality() {
        let prefs = UserMediaPreferences {
            video_quality: "1080".into(),
            ..UserMediaPreferences::default()
        };
        assert_eq!(prefs.to_asset_policy().video, "1080");
    }
}
