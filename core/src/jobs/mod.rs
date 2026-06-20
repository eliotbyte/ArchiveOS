use archiveos_contract::VaultError;
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: Uuid,
    pub job_type: String,
    pub target_vault: String,
    pub input: String,
    pub status: String,
    pub lease_until: Option<String>,
    pub attempts: i32,
    pub created_at: String,
}

pub fn create_job(
    conn: &Connection,
    job_type: &str,
    target_vault: &str,
    input: &str,
) -> Result<Job, VaultError> {
    let job = Job {
        id: Uuid::new_v4(),
        job_type: job_type.to_string(),
        target_vault: target_vault.to_string(),
        input: input.to_string(),
        status: "pending".to_string(),
        lease_until: None,
        attempts: 0,
        created_at: Utc::now().to_rfc3339(),
    };

    conn.execute(
        "INSERT INTO job (id, type, target_vault, input, status, lease_until, attempts, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0, ?6)",
        params![
            job.id.to_string(),
            job.job_type,
            job.target_vault,
            job.input,
            job.status,
            job.created_at,
        ],
    )
    .map_err(db_err)?;

    Ok(job)
}

pub fn claim_job(conn: &Connection, lease_secs: i64) -> Result<Option<Job>, VaultError> {
    let lease_until = (Utc::now() + Duration::seconds(lease_secs)).to_rfc3339();

    conn.query_row(
        "UPDATE job SET status = 'claimed', lease_until = ?1, attempts = attempts + 1
         WHERE id = (
           SELECT id FROM job WHERE status = 'pending' ORDER BY created_at LIMIT 1
         )
         RETURNING id, type, target_vault, input, status, lease_until, attempts, created_at",
        [lease_until],
        map_job,
    )
    .optional()
    .map_err(db_err)
}

pub fn get_job(conn: &Connection, job_id: Uuid) -> Result<Option<Job>, VaultError> {
    conn.query_row(
        "SELECT id, type, target_vault, input, status, lease_until, attempts, created_at
         FROM job WHERE id = ?1",
        [job_id.to_string()],
        map_job,
    )
    .optional()
    .map_err(db_err)
}

pub fn finish_job(conn: &Connection, job_id: Uuid, status: &str) -> Result<(), VaultError> {
    let changed = conn
        .execute(
            "UPDATE job SET status = ?1, lease_until = NULL WHERE id = ?2",
            params![status, job_id.to_string()],
        )
        .map_err(db_err)?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    Ok(Job {
        id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        job_type: row.get(1)?,
        target_vault: row.get(2)?,
        input: row.get(3)?,
        status: row.get(4)?,
        lease_until: row.get(5)?,
        attempts: row.get(6)?,
        created_at: row.get(7)?,
    })
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
    fn create_and_claim_job() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let job = create_job(conn, "scan", "test", "/photos").unwrap();
        assert_eq!(job.status, "pending");

        let claimed = claim_job(conn, 300).unwrap().expect("claimed job");
        assert_eq!(claimed.id, job.id);
        assert_eq!(claimed.status, "claimed");
        assert!(claimed.lease_until.is_some());

        assert!(claim_job(conn, 300).unwrap().is_none());
    }
}
