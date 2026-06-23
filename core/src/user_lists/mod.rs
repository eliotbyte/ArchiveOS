use archiveos_contract::{AddUserListMemberInput, CreateUserListInput, VaultError};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::playback::{default_user_id, ensure_default_user};

pub const LIST_TYPE_WATCH_LATER: &str = "watch_later";
pub const LIST_TYPE_PLAYLIST: &str = "playlist";

#[derive(Debug, Clone)]
pub struct UserListRow {
    pub id: Uuid,
    pub list_type: String,
    pub title: String,
    pub last_used_at: Option<String>,
}

pub fn ensure_watch_later_list(conn: &Connection, user_id: Uuid) -> Result<Uuid, VaultError> {
    if let Some(existing) = find_user_list_by_type(conn, user_id, LIST_TYPE_WATCH_LATER)? {
        return Ok(existing);
    }
    create_user_list(
        conn,
        user_id,
        &CreateUserListInput {
            title: "Watch Later".into(),
            list_type: Some(LIST_TYPE_WATCH_LATER.into()),
        },
    )
}

pub fn find_user_list_by_type(
    conn: &Connection,
    user_id: Uuid,
    list_type: &str,
) -> Result<Option<Uuid>, VaultError> {
    conn.query_row(
        "SELECT id FROM user_list WHERE user_id = ?1 AND list_type = ?2 LIMIT 1",
        params![user_id.to_string(), list_type],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(db_err)?
    .map(|id| {
        Uuid::parse_str(&id).map_err(|e| VaultError::Database {
            detail: e.to_string(),
        })
    })
    .transpose()
}

pub fn create_user_list(
    conn: &Connection,
    user_id: Uuid,
    input: &CreateUserListInput,
) -> Result<Uuid, VaultError> {
    let list_type = input
        .list_type
        .clone()
        .unwrap_or_else(|| LIST_TYPE_PLAYLIST.into());
    if list_type == LIST_TYPE_WATCH_LATER {
        if let Some(existing) = find_user_list_by_type(conn, user_id, LIST_TYPE_WATCH_LATER)? {
            return Ok(existing);
        }
    }
    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO user_list (id, user_id, list_type, title, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            id.to_string(),
            user_id.to_string(),
            list_type,
            input.title,
            now,
        ],
    )
    .map_err(db_err)?;
    Ok(id)
}

pub fn list_user_lists(conn: &Connection, user_id: Uuid) -> Result<Vec<UserListRow>, VaultError> {
    let mut stmt = conn
        .prepare(
            "SELECT ul.id, ul.list_type, ul.title, MAX(ulm.added_at) AS last_used_at
             FROM user_list ul
             LEFT JOIN user_list_member ulm
               ON ulm.list_id = ul.id AND ulm.status = 'active'
             WHERE ul.user_id = ?1
             GROUP BY ul.id
             ORDER BY
               CASE WHEN last_used_at IS NULL THEN 1 ELSE 0 END,
               last_used_at DESC,
               ul.title COLLATE NOCASE ASC",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([user_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(db_err)?;

    let mut items = Vec::new();
    for row in rows {
        let (id, list_type, title, last_used_at) = row.map_err(db_err)?;
        items.push(UserListRow {
            id: Uuid::parse_str(&id).map_err(|e| VaultError::Database {
                detail: e.to_string(),
            })?,
            list_type,
            title,
            last_used_at,
        });
    }
    Ok(items)
}

pub fn get_user_list(
    conn: &Connection,
    list_id: Uuid,
) -> Result<Option<UserListRow>, VaultError> {
    conn.query_row(
        "SELECT id, list_type, title FROM user_list WHERE id = ?1",
        [list_id.to_string()],
        |row| {
            Ok(UserListRow {
                id: list_id,
                list_type: row.get(1)?,
                title: row.get(2)?,
                last_used_at: None,
            })
        },
    )
    .optional()
    .map_err(db_err)
}

pub fn add_user_list_member(
    conn: &Connection,
    list_id: Uuid,
    input: &AddUserListMemberInput,
) -> Result<(), VaultError> {
    let list = get_user_list(conn, list_id)?.ok_or(VaultError::NotFound)?;
    let _ = list;

    let now = Utc::now().to_rfc3339();
    let next_position: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM user_list_member WHERE list_id = ?1 AND status = 'active'",
            [list_id.to_string()],
            |row| row.get(0),
        )
        .map_err(db_err)?;

    conn.execute(
        "INSERT INTO user_list_member (list_id, entity_id, position, added_at, status)
         VALUES (?1, ?2, ?3, ?4, 'active')
         ON CONFLICT(list_id, entity_id) DO UPDATE SET
            status = 'active',
            added_at = excluded.added_at",
        params![
            list_id.to_string(),
            input.entity_id.to_string(),
            next_position,
            now,
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn remove_user_list_member(
    conn: &Connection,
    list_id: Uuid,
    entity_id: Uuid,
) -> Result<(), VaultError> {
    let changed = conn
        .execute(
            "UPDATE user_list_member SET status = 'removed'
             WHERE list_id = ?1 AND entity_id = ?2 AND status = 'active'",
            params![list_id.to_string(), entity_id.to_string()],
        )
        .map_err(db_err)?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

pub fn reorder_user_list_members(
    conn: &Connection,
    list_id: Uuid,
    entity_ids: &[Uuid],
) -> Result<(), VaultError> {
    if get_user_list(conn, list_id)?.is_none() {
        return Err(VaultError::NotFound);
    }
    for (position, entity_id) in entity_ids.iter().enumerate() {
        conn.execute(
            "UPDATE user_list_member SET position = ?3
             WHERE list_id = ?1 AND entity_id = ?2 AND status = 'active'",
            params![
                list_id.to_string(),
                entity_id.to_string(),
                position as i32,
            ],
        )
        .map_err(db_err)?;
    }
    Ok(())
}

pub fn count_user_list_members(conn: &Connection, list_id: Uuid) -> Result<i32, VaultError> {
    conn.query_row(
        "SELECT COUNT(*) FROM user_list_member WHERE list_id = ?1 AND status = 'active'",
        [list_id.to_string()],
        |row| row.get(0),
    )
    .map_err(db_err)
}

pub fn list_user_list_entity_ids(
    conn: &Connection,
    list_id: Uuid,
) -> Result<Vec<(Uuid, i32)>, VaultError> {
    let mut stmt = conn
        .prepare(
            "SELECT entity_id, position FROM user_list_member
             WHERE list_id = ?1 AND status = 'active'
             ORDER BY position ASC",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([list_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
        })
        .map_err(db_err)?;

    let mut items = Vec::new();
    for row in rows {
        let (entity_id, position) = row.map_err(db_err)?;
        items.push((
            Uuid::parse_str(&entity_id).map_err(|e| VaultError::Database {
                detail: e.to_string(),
            })?,
            position,
        ));
    }
    Ok(items)
}

pub fn init_user_lists(conn: &Connection) -> Result<(), VaultError> {
    let user_id = ensure_default_user(conn)?;
    ensure_watch_later_list(conn, user_id)?;
    Ok(())
}

pub fn default_user() -> Uuid {
    default_user_id()
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archiveos_contract::AddUserListMemberInput;
    use crate::import::db::insert_entity;
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn insert_entity_row(conn: &rusqlite::Connection, entity_id: Uuid) {
        let now = Utc::now().to_rfc3339();
        insert_entity(conn, entity_id, None, Some("video/mp4"), 0, "present", &now, None)
            .unwrap();
    }

    #[test]
    fn list_user_lists_orders_recently_used_playlists_first() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let user_id = ensure_default_user(conn).unwrap();
        let _watch_later = ensure_watch_later_list(conn, user_id).unwrap();

        let music_id = create_user_list(
            conn,
            user_id,
            &CreateUserListInput {
                title: "Music".into(),
                list_type: Some(LIST_TYPE_PLAYLIST.into()),
            },
        )
        .unwrap();
        let gaming_id = create_user_list(
            conn,
            user_id,
            &CreateUserListInput {
                title: "Gaming".into(),
                list_type: Some(LIST_TYPE_PLAYLIST.into()),
            },
        )
        .unwrap();
        let empty_id = create_user_list(
            conn,
            user_id,
            &CreateUserListInput {
                title: "Empty".into(),
                list_type: Some(LIST_TYPE_PLAYLIST.into()),
            },
        )
        .unwrap();

        let entity_a = Uuid::new_v4();
        let entity_b = Uuid::new_v4();
        insert_entity_row(conn, entity_a);
        insert_entity_row(conn, entity_b);
        add_user_list_member(
            conn,
            gaming_id,
            &AddUserListMemberInput { entity_id: entity_a },
        )
        .unwrap();
        add_user_list_member(
            conn,
            music_id,
            &AddUserListMemberInput { entity_id: entity_b },
        )
        .unwrap();

        let playlists: Vec<_> = list_user_lists(conn, user_id)
            .unwrap()
            .into_iter()
            .filter(|list| list.list_type == LIST_TYPE_PLAYLIST)
            .collect();

        assert_eq!(playlists.len(), 3);
        assert_eq!(playlists[0].id, music_id);
        assert_eq!(playlists[1].id, gaming_id);
        assert_eq!(playlists[2].id, empty_id);
    }
}
