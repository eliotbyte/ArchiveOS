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

/// Requeue jobs whose lease expired while claimed/running.
pub fn requeue_expired_jobs(conn: &Connection) -> Result<u32, VaultError> {
    let now = Utc::now().to_rfc3339();
    let changed = conn
        .execute(
            "UPDATE job SET status = 'pending', lease_until = NULL
             WHERE status IN ('claimed', 'running')
               AND lease_until IS NOT NULL
               AND lease_until < ?1",
            [now],
        )
        .map_err(db_err)?;
    Ok(changed as u32)
}

pub fn claim_job(
    conn: &Connection,
    lease_secs: i64,
    job_type: Option<&str>,
) -> Result<Option<Job>, VaultError> {
    requeue_expired_jobs(conn)?;
    let lease_until = (Utc::now() + Duration::seconds(lease_secs)).to_rfc3339();

    let job = if let Some(job_type) = job_type {
        conn.query_row(
            "UPDATE job SET status = 'running', lease_until = ?1, attempts = attempts + 1
             WHERE id = (
               SELECT id FROM job
               WHERE status = 'pending' AND type = ?2
               ORDER BY created_at LIMIT 1
             )
             RETURNING id, type, target_vault, input, status, lease_until, attempts, created_at",
            params![lease_until, job_type],
            map_job,
        )
        .optional()
        .map_err(db_err)?
    } else {
        conn.query_row(
            "UPDATE job SET status = 'running', lease_until = ?1, attempts = attempts + 1
             WHERE id = (
               SELECT id FROM job WHERE status = 'pending' ORDER BY created_at LIMIT 1
             )
             RETURNING id, type, target_vault, input, status, lease_until, attempts, created_at",
            [lease_until],
            map_job,
        )
        .optional()
        .map_err(db_err)?
    };

    Ok(job)
}

pub fn heartbeat_job(conn: &Connection, job_id: Uuid, lease_secs: i64) -> Result<(), VaultError> {
    let lease_until = (Utc::now() + Duration::seconds(lease_secs)).to_rfc3339();
    let changed = conn
        .execute(
            "UPDATE job SET lease_until = ?1
             WHERE id = ?2 AND status IN ('claimed', 'running')",
            params![lease_until, job_id.to_string()],
        )
        .map_err(db_err)?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
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

#[derive(Debug, Clone, Default)]
pub struct JobListFilter<'a> {
    pub status: Option<&'a str>,
    pub job_type: Option<&'a str>,
    pub limit: i32,
}

pub fn list_jobs(conn: &Connection, filter: JobListFilter<'_>) -> Result<Vec<Job>, VaultError> {
    let limit = filter.limit.clamp(1, 500);
    let mut sql = String::from(
        "SELECT id, type, target_vault, input, status, lease_until, attempts, created_at
         FROM job WHERE 1=1",
    );
    let mut params: Vec<String> = Vec::new();
    if let Some(status) = filter.status {
        params.push(status.to_string());
        sql.push_str(&format!(" AND status = ?{}", params.len()));
    }
    if let Some(job_type) = filter.job_type {
        params.push(job_type.to_string());
        sql.push_str(&format!(" AND type = ?{}", params.len()));
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ");
    sql.push_str(&limit.to_string());

    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let rows = if params.is_empty() {
        stmt.query_map([], map_job).map_err(db_err)?
    } else if params.len() == 1 {
        stmt.query_map([&params[0]], map_job).map_err(db_err)?
    } else {
        stmt.query_map(rusqlite::params![params[0], params[1]], map_job)
            .map_err(db_err)?
    };
    rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

pub fn retry_job(conn: &Connection, job_id: Uuid) -> Result<Job, VaultError> {
    let job = get_job(conn, job_id)?.ok_or(VaultError::NotFound)?;
    if job.status == "running" {
        return Err(VaultError::InvalidLayout {
            detail: format!("job {} is still running", job_id),
        });
    }
    conn.execute(
        "UPDATE job SET status = 'pending', lease_until = NULL WHERE id = ?1",
        [job_id.to_string()],
    )
    .map_err(db_err)?;
    get_job(conn, job_id)?.ok_or(VaultError::NotFound)
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
    use chrono::{Duration, Utc};
    use tempfile::tempdir;

    #[test]
    fn create_and_claim_job() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let job = create_job(conn, "scan", "test", "/photos").unwrap();
        assert_eq!(job.status, "pending");

        let claimed = claim_job(conn, 300, None).unwrap().expect("claimed job");
        assert_eq!(claimed.id, job.id);
        assert_eq!(claimed.status, "running");
        assert!(claimed.lease_until.is_some());

        assert!(claim_job(conn, 300, None).unwrap().is_none());
    }

    #[test]
    fn claim_by_type_skips_other_jobs() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let scan = create_job(conn, "scan", "test", "/photos").unwrap();
        let yt = create_job(conn, "yt-dlp", "test", "https://youtube.com/watch?v=x").unwrap();

        let claimed = claim_job(conn, 300, Some("yt-dlp"))
            .unwrap()
            .expect("yt job");
        assert_eq!(claimed.id, yt.id);
        assert_ne!(claimed.id, scan.id);

        let scan_claimed = claim_job(conn, 300, None).unwrap().expect("scan job");
        assert_eq!(scan_claimed.id, scan.id);
    }

    #[test]
    fn heartbeat_extends_lease() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let job = create_job(conn, "yt-dlp", "test", "url").unwrap();
        claim_job(conn, 60, None).unwrap();

        heartbeat_job(conn, job.id, 600).unwrap();
        let updated = get_job(conn, job.id).unwrap().unwrap();
        let lease = updated.lease_until.unwrap();
        let parsed = chrono::DateTime::parse_from_rfc3339(&lease).unwrap();
        assert!(parsed > Utc::now() + Duration::seconds(500));
    }

    #[test]
    fn expired_lease_requeued_on_claim() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let job = create_job(conn, "yt-dlp", "test", "url").unwrap();
        let past = (Utc::now() - Duration::seconds(60)).to_rfc3339();
        conn.execute(
            "UPDATE job SET status = 'running', lease_until = ?1, attempts = 1 WHERE id = ?2",
            params![past, job.id.to_string()],
        )
        .unwrap();

        requeue_expired_jobs(conn).unwrap();
        let reclaimed = claim_job(conn, 300, Some("yt-dlp"))
            .unwrap()
            .expect("reclaimed");
        assert_eq!(reclaimed.id, job.id);
        assert_eq!(reclaimed.status, "running");
    }

    #[test]
    fn list_and_retry_job() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let job = create_job(conn, "yt-dlp", "test", "url").unwrap();
        finish_job(conn, job.id, "failed").unwrap();

        let listed = list_jobs(
            conn,
            JobListFilter {
                status: Some("failed"),
                job_type: Some("yt-dlp"),
                limit: 10,
            },
        )
        .unwrap();
        assert_eq!(listed.len(), 1);

        let retried = retry_job(conn, job.id).unwrap();
        assert_eq!(retried.status, "pending");
    }
}
