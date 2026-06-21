use archiveos_contract::{ManifestSourceRef, VaultError};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub fn find_entity_by_content_hash(
    conn: &Connection,
    content_hash: &str,
) -> Result<Option<Uuid>, VaultError> {
    conn.query_row(
        "SELECT id FROM entity WHERE content_hash = ?1",
        [content_hash],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(db_err)?
    .map(|id| Uuid::parse_str(&id).map_err(|e| VaultError::InvalidLayout { detail: e.to_string() }))
    .transpose()
}

pub fn find_entity_by_source_ref(
    conn: &Connection,
    source: &str,
    kind: &str,
    external_id: &str,
) -> Result<Option<Uuid>, VaultError> {
    conn.query_row(
        "SELECT entity_id FROM source_ref WHERE source = ?1 AND kind = ?2 AND external_id = ?3",
        params![source, kind, external_id],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(db_err)?
    .map(|id| Uuid::parse_str(&id).map_err(|e| VaultError::InvalidLayout { detail: e.to_string() }))
    .transpose()
}

#[expect(clippy::too_many_arguments, reason = "maps 1:1 to entity table columns")]
pub fn insert_entity(
    conn: &Connection,
    id: Uuid,
    content_hash: Option<&str>,
    mime: Option<&str>,
    size: u64,
    status: &str,
    added_at: &str,
    created_at: Option<&str>,
) -> Result<(), VaultError> {
    conn.execute(
        "INSERT INTO entity (id, content_hash, mime, size, status, added_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id.to_string(),
            content_hash,
            mime,
            i64::try_from(size).unwrap_or(i64::MAX),
            status,
            added_at,
            created_at,
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn update_entity_content(
    conn: &Connection,
    id: Uuid,
    content_hash: &str,
    mime: &str,
    size: u64,
) -> Result<(), VaultError> {
    conn.execute(
        "UPDATE entity SET content_hash = ?1, mime = ?2, size = ?3, status = 'present'
         WHERE id = ?4",
        params![
            content_hash,
            mime,
            i64::try_from(size).unwrap_or(i64::MAX),
            id.to_string(),
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn update_entity_created_at(
    conn: &Connection,
    id: Uuid,
    created_at: &str,
) -> Result<(), VaultError> {
    conn.execute(
        "UPDATE entity SET created_at = ?1 WHERE id = ?2",
        params![created_at, id.to_string()],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn insert_collection(
    conn: &Connection,
    id: Uuid,
    collection_type: &str,
    title: &str,
) -> Result<(), VaultError> {
    conn.execute(
        "INSERT INTO collection (id, type, title, content_fingerprint) VALUES (?1, ?2, ?3, NULL)",
        params![id.to_string(), collection_type, title],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn find_collection_by_fingerprint(
    conn: &Connection,
    fingerprint: &str,
) -> Result<Option<Uuid>, VaultError> {
    conn.query_row(
        "SELECT id FROM collection WHERE content_fingerprint = ?1",
        [fingerprint],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(db_err)?
    .map(|id| {
        Uuid::parse_str(&id).map_err(|e| VaultError::InvalidLayout {
            detail: e.to_string(),
        })
    })
    .transpose()
}

pub fn set_collection_fingerprint(
    conn: &Connection,
    id: Uuid,
    fingerprint: &str,
) -> Result<(), VaultError> {
    conn.execute(
        "UPDATE collection SET content_fingerprint = ?1 WHERE id = ?2",
        params![fingerprint, id.to_string()],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn collection_has_source_path(
    conn: &Connection,
    collection_id: Uuid,
    path: &str,
) -> Result<bool, VaultError> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM source_ref
             WHERE entity_id = ?1 AND source = 'inbox' AND kind = 'folder' AND external_id = ?2",
            params![collection_id.to_string(), path],
            |_| Ok(()),
        )
        .optional()
        .map_err(db_err)?
        .is_some())
}

pub fn add_collection_source_path(
    conn: &Connection,
    collection_id: Uuid,
    path: &str,
) -> Result<(), VaultError> {
    if collection_has_source_path(conn, collection_id, path)? {
        return Ok(());
    }

    if let Some(existing) = find_entity_by_source_ref(conn, "inbox", "folder", path)?
        && existing != collection_id
    {
        return Ok(());
    }

    insert_source_ref(
        conn,
        Uuid::new_v4(),
        collection_id,
        "inbox",
        "folder",
        path,
        None,
        "live",
    )
}

#[expect(clippy::too_many_arguments, reason = "maps 1:1 to source_ref table columns")]
pub fn insert_source_ref(
    conn: &Connection,
    id: Uuid,
    entity_id: Uuid,
    source: &str,
    kind: &str,
    external_id: &str,
    url: Option<&str>,
    status: &str,
) -> Result<(), VaultError> {
    conn.execute(
        "INSERT INTO source_ref (id, entity_id, source, kind, external_id, url, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id.to_string(),
            entity_id.to_string(),
            source,
            kind,
            external_id,
            url,
            status,
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn upsert_source_ref(
    conn: &Connection,
    entity_id: Uuid,
    source_ref: &ManifestSourceRef,
) -> Result<(), VaultError> {
    if let Some(existing_id) = find_entity_by_source_ref(
        conn,
        &source_ref.source,
        &source_ref.kind,
        &source_ref.external_id,
    )? {
        if existing_id != entity_id {
            conn.execute(
                "UPDATE source_ref SET entity_id = ?1, url = ?2, status = 'live'
                 WHERE source = ?3 AND kind = ?4 AND external_id = ?5",
                params![
                    entity_id.to_string(),
                    source_ref.url,
                    source_ref.source,
                    source_ref.kind,
                    source_ref.external_id,
                ],
            )
            .map_err(db_err)?;
            return Ok(());
        }
    } else {
        insert_source_ref(
            conn,
            Uuid::new_v4(),
            entity_id,
            &source_ref.source,
            &source_ref.kind,
            &source_ref.external_id,
            source_ref.url.as_deref(),
            "live",
        )?;
    }
    Ok(())
}

pub fn upsert_metadata(
    conn: &Connection,
    entity_id: Uuid,
    key: &str,
    value: &str,
    provenance: &str,
) -> Result<(), VaultError> {
    conn.execute(
        "INSERT INTO metadata (entity_id, key, value, provenance)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(entity_id, key, provenance) DO UPDATE SET value = excluded.value",
        params![entity_id.to_string(), key, value, provenance],
    )
    .map_err(db_err)?;
    Ok(())
}

pub fn link_collection_member(
    conn: &Connection,
    collection_id: Uuid,
    source: &str,
    kind: &str,
    external_id: &str,
    position: i32,
) -> Result<bool, VaultError> {
    let Some(entity_id) = find_entity_by_source_ref(conn, source, kind, external_id)? else {
        return Ok(false);
    };

    link_collection_member_direct(conn, collection_id, entity_id, position)
}

pub fn link_collection_member_direct(
    conn: &Connection,
    collection_id: Uuid,
    entity_id: Uuid,
    position: i32,
) -> Result<bool, VaultError> {
    conn.execute(
        "INSERT INTO collection_member (collection_id, entity_id, position)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(collection_id, entity_id) DO UPDATE SET position = excluded.position",
        params![collection_id.to_string(), entity_id.to_string(), position],
    )
    .map_err(db_err)?;
    Ok(true)
}

pub fn link_entity_relation(
    conn: &Connection,
    from_entity_id: Uuid,
    to_entity_id: Uuid,
    relation: &str,
) -> Result<(), VaultError> {
    conn.execute(
        "INSERT INTO entity_relation (from_entity_id, to_entity_id, relation)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(from_entity_id, to_entity_id, relation) DO NOTHING",
        params![
            from_entity_id.to_string(),
            to_entity_id.to_string(),
            relation,
        ],
    )
    .map_err(db_err)?;
    Ok(())
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}
