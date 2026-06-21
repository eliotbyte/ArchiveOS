use archiveos_contract::VaultError;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFailure {
    pub id: Uuid,
    pub job_id: Option<Uuid>,
    pub source: String,
    pub kind: String,
    pub external_id: String,
    pub url: Option<String>,
    pub stage: String,
    pub error_kind: String,
    pub message: String,
    pub retryable: bool,
    pub attempts: i32,
    pub last_seen_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct RecordFailureInput {
    pub job_id: Option<Uuid>,
    pub source: String,
    pub kind: String,
    pub external_id: String,
    pub url: Option<String>,
    pub stage: String,
    pub error_kind: String,
    pub message: String,
    pub retryable: bool,
}

pub fn record_failure(conn: &Connection, input: &RecordFailureInput) -> Result<SourceFailure, VaultError> {
    let now = Utc::now().to_rfc3339();
    let existing: Option<(String, i32)> = conn
        .query_row(
            "SELECT id, attempts FROM source_failure
             WHERE source = ?1 AND kind = ?2 AND external_id = ?3 AND stage = ?4",
            params![input.source, input.kind, input.external_id, input.stage],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(db_err)?;

    if let Some((id, attempts)) = existing {
        let attempts = attempts + 1;
        conn.execute(
            "UPDATE source_failure
             SET job_id = ?1, url = ?2, error_kind = ?3, message = ?4,
                 retryable = ?5, attempts = ?6, last_seen_at = ?7
             WHERE id = ?8",
            params![
                input.job_id.map(|j| j.to_string()),
                input.url,
                input.error_kind,
                input.message,
                i32::from(input.retryable),
                attempts,
                now,
                id,
            ],
        )
        .map_err(db_err)?;
        return get_failure(conn, Uuid::parse_str(&id).map_err(parse_err)?)?.ok_or(VaultError::NotFound);
    }

    let id = Uuid::new_v4();
    conn.execute(
        "INSERT INTO source_failure (
            id, job_id, source, kind, external_id, url, stage, error_kind,
            message, retryable, attempts, last_seen_at, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?11)",
        params![
            id.to_string(),
            input.job_id.map(|j| j.to_string()),
            input.source,
            input.kind,
            input.external_id,
            input.url,
            input.stage,
            input.error_kind,
            input.message,
            i32::from(input.retryable),
            now,
        ],
    )
    .map_err(db_err)?;

    get_failure(conn, id)?.ok_or(VaultError::NotFound)
}

pub fn list_failures(
    conn: &Connection,
    source: Option<&str>,
    kind: Option<&str>,
) -> Result<Vec<SourceFailure>, VaultError> {
    let mut sql = String::from(
        "SELECT id, job_id, source, kind, external_id, url, stage, error_kind,
                message, retryable, attempts, last_seen_at, created_at
         FROM source_failure WHERE 1=1",
    );
    let mut binds: Vec<String> = Vec::new();
    if let Some(source) = source {
        sql.push_str(" AND source = ?");
        binds.push(source.to_string());
    }
    if let Some(kind) = kind {
        sql.push_str(" AND kind = ?");
        binds.push(kind.to_string());
    }
    sql.push_str(" ORDER BY last_seen_at DESC");

    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let rows = match binds.len() {
        0 => stmt.query_map([], map_failure).map_err(db_err)?,
        1 => stmt
            .query_map(params![binds[0]], map_failure)
            .map_err(db_err)?,
        2 => stmt
            .query_map(params![binds[0], binds[1]], map_failure)
            .map_err(db_err)?,
        _ => unreachable!(),
    };

    rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

pub fn get_failure(conn: &Connection, id: Uuid) -> Result<Option<SourceFailure>, VaultError> {
    conn.query_row(
        "SELECT id, job_id, source, kind, external_id, url, stage, error_kind,
                message, retryable, attempts, last_seen_at, created_at
         FROM source_failure WHERE id = ?1",
        [id.to_string()],
        map_failure,
    )
    .optional()
    .map_err(db_err)
}

fn map_failure(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceFailure> {
    let job_id: Option<String> = row.get(1)?;
    Ok(SourceFailure {
        id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        job_id: job_id
            .map(|id| Uuid::parse_str(&id))
            .transpose()
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
            })?,
        source: row.get(2)?,
        kind: row.get(3)?,
        external_id: row.get(4)?,
        url: row.get(5)?,
        stage: row.get(6)?,
        error_kind: row.get(7)?,
        message: row.get(8)?,
        retryable: row.get::<_, i32>(9)? != 0,
        attempts: row.get(10)?,
        last_seen_at: row.get(11)?,
        created_at: row.get(12)?,
    })
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

fn parse_err(err: uuid::Error) -> VaultError {
    VaultError::InvalidLayout {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vault;
    use tempfile::tempdir;

    #[test]
    fn record_and_list_failure() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let failure = record_failure(
            conn,
            &RecordFailureInput {
                job_id: None,
                source: "youtube".into(),
                kind: "video".into(),
                external_id: "abc123".into(),
                url: Some("https://youtube.com/watch?v=abc123".into()),
                stage: "download".into(),
                error_kind: "unavailable".into(),
                message: "video unavailable".into(),
                retryable: true,
            },
        )
        .unwrap();
        assert_eq!(failure.attempts, 1);

        let updated = record_failure(
            conn,
            &RecordFailureInput {
                job_id: None,
                source: "youtube".into(),
                kind: "video".into(),
                external_id: "abc123".into(),
                url: None,
                stage: "download".into(),
                error_kind: "unavailable".into(),
                message: "still unavailable".into(),
                retryable: true,
            },
        )
        .unwrap();
        assert_eq!(updated.attempts, 2);

        let listed = list_failures(conn, Some("youtube"), Some("video")).unwrap();
        assert_eq!(listed.len(), 1);
    }
}
