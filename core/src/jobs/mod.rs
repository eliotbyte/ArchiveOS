use archiveos_contract::{JobProgress, VaultError, YtdlpJobInput};
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

const JOB_SELECT: &str =
    "SELECT id, type, target_vault, input, status, lease_until, attempts, created_at, progress_json, parent_job_id
     FROM job";

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: Uuid,
    pub job_type: String,
    pub target_vault: String,
    pub input: String,
    pub status: String,
    pub lease_until: Option<String>,
    pub attempts: i32,
    pub created_at: String,
    pub progress: Option<JobProgress>,
    pub parent_job_id: Option<Uuid>,
}

pub struct CreateJobOptions<'a> {
    pub job_type: &'a str,
    pub target_vault: &'a str,
    pub input: &'a str,
    pub parent_job_id: Option<Uuid>,
}

pub fn create_job(
    conn: &Connection,
    job_type: &str,
    target_vault: &str,
    input: &str,
) -> Result<Job, VaultError> {
    create_job_with_options(
        conn,
        CreateJobOptions {
            job_type,
            target_vault,
            input,
            parent_job_id: None,
        },
    )
}

pub fn create_job_with_options(
    conn: &Connection,
    options: CreateJobOptions<'_>,
) -> Result<Job, VaultError> {
    let job = Job {
        id: Uuid::new_v4(),
        job_type: options.job_type.to_string(),
        target_vault: options.target_vault.to_string(),
        input: options.input.to_string(),
        status: "pending".to_string(),
        lease_until: None,
        attempts: 0,
        created_at: Utc::now().to_rfc3339(),
        progress: None,
        parent_job_id: options.parent_job_id,
    };

    conn.execute(
        "INSERT INTO job (id, type, target_vault, input, status, lease_until, attempts, created_at, progress_json, parent_job_id)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0, ?6, NULL, ?7)",
        params![
            job.id.to_string(),
            job.job_type,
            job.target_vault,
            job.input,
            job.status,
            job.created_at,
            job.parent_job_id.map(|id| id.to_string()),
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
            &format!(
                "UPDATE job SET status = 'running', lease_until = ?1, attempts = attempts + 1
                 WHERE id = (
                   SELECT id FROM job
                   WHERE status = 'pending' AND type = ?2
                   ORDER BY created_at LIMIT 1
                 )
                 RETURNING {JOB_SELECT_COLUMNS}",
                JOB_SELECT_COLUMNS = job_returning_columns()
            ),
            params![lease_until, job_type],
            map_job,
        )
        .optional()
        .map_err(db_err)?
    } else {
        conn.query_row(
            &format!(
                "UPDATE job SET status = 'running', lease_until = ?1, attempts = attempts + 1
                 WHERE id = (
                   SELECT id FROM job WHERE status = 'pending' ORDER BY created_at LIMIT 1
                 )
                 RETURNING {JOB_SELECT_COLUMNS}",
                JOB_SELECT_COLUMNS = job_returning_columns()
            ),
            [lease_until],
            map_job,
        )
        .optional()
        .map_err(db_err)?
    };

    Ok(job)
}

pub fn heartbeat_job(
    conn: &Connection,
    job_id: Uuid,
    lease_secs: i64,
    progress: Option<&JobProgress>,
) -> Result<bool, VaultError> {
    let job = get_job(conn, job_id)?.ok_or(VaultError::NotFound)?;
    if job.status == "cancelled" {
        return Ok(true);
    }
    if job.status != "claimed" && job.status != "running" {
        return Err(VaultError::NotFound);
    }

    let lease_until = (Utc::now() + Duration::seconds(lease_secs)).to_rfc3339();
    let progress_json = progress
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| VaultError::InvalidLayout {
            detail: e.to_string(),
        })?;

    let changed = if let Some(json) = progress_json {
        conn.execute(
            "UPDATE job SET lease_until = ?1, progress_json = ?2
             WHERE id = ?3 AND status IN ('claimed', 'running')",
            params![lease_until, json, job_id.to_string()],
        )
        .map_err(db_err)?
    } else {
        conn.execute(
            "UPDATE job SET lease_until = ?1
             WHERE id = ?2 AND status IN ('claimed', 'running')",
            params![lease_until, job_id.to_string()],
        )
        .map_err(db_err)?
    };
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(false)
}

pub fn cancel_job(conn: &Connection, job_id: Uuid) -> Result<Job, VaultError> {
    let changed = conn
        .execute(
            "UPDATE job SET status = 'cancelled', lease_until = NULL
             WHERE id = ?1 AND status IN ('pending', 'claimed', 'running')",
            [job_id.to_string()],
        )
        .map_err(db_err)?;
    if changed == 0 {
        let job = get_job(conn, job_id)?.ok_or(VaultError::NotFound)?;
        if job.status == "cancelled" {
            return Ok(job);
        }
        return Err(VaultError::InvalidLayout {
            detail: format!("job {} cannot be cancelled (status={})", job_id, job.status),
        });
    }
    get_job(conn, job_id)?.ok_or(VaultError::NotFound)
}

fn url_match_tokens(url: &str) -> Vec<String> {
    let mut tokens = vec![url.to_string()];
    if let Some(query) = url.split('?').nth(1) {
        for part in query.split('&') {
            let Some((key, value)) = part.split_once('=') else {
                continue;
            };
            if (key == "list" || key == "v") && !value.is_empty() {
                tokens.push(value.to_string());
            }
        }
    }
    tokens
}

pub fn cancel_jobs_for_source_url(conn: &Connection, url: &str) -> Result<u32, VaultError> {
    let mut total = 0u32;
    for token in url_match_tokens(url) {
        let pattern = format!("%{token}%");
        total += conn
            .execute(
                "UPDATE job SET status = 'cancelled', lease_until = NULL
                 WHERE status IN ('pending', 'claimed', 'running')
                   AND type LIKE 'yt-dlp%'
                   AND input LIKE ?1",
                [pattern],
            )
            .map_err(db_err)? as u32;
    }
    Ok(total)
}

fn retry_source_removed(conn: &Connection, input: &str) -> Result<(), VaultError> {
    let parsed = YtdlpJobInput::normalize(input)?;
    if parsed.mode == "subscription"
        && !crate::subscriptions::subscription_exists_for_url(conn, &parsed.url)?
    {
        return Err(VaultError::InvalidLayout {
            detail: "monitor for this URL was removed".into(),
        });
    }
    if crate::import::db::source_url_target_user_deleted(conn, &parsed.url)? {
        return Err(VaultError::InvalidLayout {
            detail: "source playlist or channel was removed from library".into(),
        });
    }
    Ok(())
}

pub fn update_job_progress(
    conn: &Connection,
    job_id: Uuid,
    progress: &JobProgress,
) -> Result<(), VaultError> {
    let progress_json = serde_json::to_string(progress).map_err(|e| VaultError::InvalidLayout {
        detail: e.to_string(),
    })?;
    let changed = conn
        .execute(
            "UPDATE job SET progress_json = ?1 WHERE id = ?2",
            params![progress_json, job_id.to_string()],
        )
        .map_err(db_err)?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

pub fn get_job(conn: &Connection, job_id: Uuid) -> Result<Option<Job>, VaultError> {
    conn.query_row(
        &format!("{JOB_SELECT} WHERE id = ?1"),
        [job_id.to_string()],
        map_job,
    )
    .optional()
    .map_err(db_err)
}

pub fn list_child_jobs(conn: &Connection, parent_id: Uuid) -> Result<Vec<Job>, VaultError> {
    let mut stmt = conn
        .prepare(&format!(
            "{JOB_SELECT} WHERE parent_job_id = ?1 ORDER BY created_at ASC"
        ))
        .map_err(db_err)?;
    let rows = stmt
        .query_map([parent_id.to_string()], map_job)
        .map_err(db_err)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

#[derive(Debug, Clone, Default)]
pub struct JobListFilter<'a> {
    pub status: Option<&'a str>,
    pub job_type: Option<&'a str>,
    pub root_only: bool,
    pub limit: i32,
}

pub fn list_jobs(conn: &Connection, filter: JobListFilter<'_>) -> Result<Vec<Job>, VaultError> {
    let limit = filter.limit.clamp(1, 500);
    let mut sql = format!("{JOB_SELECT} WHERE 1=1");
    let mut bind_values: Vec<String> = Vec::new();
    if filter.root_only {
        sql.push_str(" AND parent_job_id IS NULL");
    }
    if let Some(status) = filter.status {
        bind_values.push(status.to_string());
        sql.push_str(&format!(" AND status = ?{}", bind_values.len()));
    }
    if let Some(job_type) = filter.job_type {
        bind_values.push(job_type.to_string());
        sql.push_str(&format!(" AND type = ?{}", bind_values.len()));
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ");
    sql.push_str(&limit.to_string());

    let mut stmt = conn.prepare(&sql).map_err(db_err)?;
    let rows = match bind_values.len() {
        0 => stmt.query_map([], map_job).map_err(db_err)?,
        1 => stmt
            .query_map([&bind_values[0]], map_job)
            .map_err(db_err)?,
        _ => stmt
            .query_map(rusqlite::params![bind_values[0], bind_values[1]], map_job)
            .map_err(db_err)?,
    };
    rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
}

pub fn retry_job(conn: &Connection, job_id: Uuid) -> Result<Job, VaultError> {
    let job = get_job(conn, job_id)?.ok_or(VaultError::NotFound)?;
    if job.status == "running" || job.status == "claimed" {
        return Err(VaultError::InvalidLayout {
            detail: format!("job {} is still running", job_id),
        });
    }
    if job.job_type.starts_with("yt-dlp") {
        retry_source_removed(conn, &job.input)?;
    }
    conn.execute(
        "UPDATE job SET status = 'pending', lease_until = NULL, progress_json = NULL WHERE id = ?1",
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

fn job_returning_columns() -> &'static str {
    "id, type, target_vault, input, status, lease_until, attempts, created_at, progress_json, parent_job_id"
}

fn map_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    let progress_raw: Option<String> = row.get(8)?;
    let progress = progress_raw
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|json| serde_json::from_str(json))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let parent_raw: Option<String> = row.get(9)?;
    let parent_job_id = parent_raw
        .filter(|s| !s.is_empty())
        .map(|id| Uuid::parse_str(&id))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
        })?;

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
        progress,
        parent_job_id,
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
    use archiveos_contract::JobProgressStep;
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
    fn heartbeat_extends_lease_and_stores_progress() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let job = create_job(conn, "yt-dlp", "test", "url").unwrap();
        claim_job(conn, 60, None).unwrap();

        let progress = JobProgress {
            phase: "downloading".into(),
            current: Some(2),
            total: Some(5),
            label: Some("video title".into()),
            percent: Some(42.0),
            steps: vec![JobProgressStep {
                id: "v1".into(),
                label: "Video 1".into(),
                status: "done".into(),
                percent: None,
            }],
        };
        assert!(!heartbeat_job(conn, job.id, 600, Some(&progress)).unwrap());
        let updated = get_job(conn, job.id).unwrap().unwrap();
        let lease = updated.lease_until.unwrap();
        let parsed = chrono::DateTime::parse_from_rfc3339(&lease).unwrap();
        assert!(parsed > Utc::now() + Duration::seconds(500));
        let stored = updated.progress.unwrap();
        assert_eq!(stored.current, Some(2));
        assert_eq!(stored.total, Some(5));
    }

    #[test]
    fn child_jobs_linked_to_parent() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let parent = create_job(conn, "yt-dlp", "test", "url").unwrap();
        let child = create_job_with_options(
            conn,
            CreateJobOptions {
                job_type: "preview",
                target_vault: "test",
                input: r#"{"entity_id":"00000000-0000-0000-0000-000000000001"}"#,
                parent_job_id: Some(parent.id),
            },
        )
        .unwrap();
        assert_eq!(child.parent_job_id, Some(parent.id));

        let children = list_child_jobs(conn, parent.id).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child.id);

        let root_only = list_jobs(
            conn,
            JobListFilter {
                root_only: true,
                limit: 50,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(root_only.len(), 1);
        assert_eq!(root_only[0].id, parent.id);
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
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(listed.len(), 1);

        let retried = retry_job(conn, job.id).unwrap();
        assert_eq!(retried.status, "pending");
    }

    #[test]
    fn cancel_job_stops_running_and_heartbeat_reports_cancelled() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let job = create_job(conn, "yt-dlp", "test", "https://youtube.com/playlist?list=PL123").unwrap();
        claim_job(conn, 60, None).unwrap();

        let cancelled = cancel_job(conn, job.id).unwrap();
        assert_eq!(cancelled.status, "cancelled");

        assert!(heartbeat_job(conn, job.id, 60, None).unwrap());
    }

    #[test]
    fn cancel_jobs_for_source_url_matches_playlist_id() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let url = "https://www.youtube.com/playlist?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI";
        let job = create_job(conn, "yt-dlp", "test", url).unwrap();
        claim_job(conn, 60, None).unwrap();

        let count = cancel_jobs_for_source_url(conn, url).unwrap();
        assert_eq!(count, 1);
        let updated = get_job(conn, job.id).unwrap().unwrap();
        assert_eq!(updated.status, "cancelled");
    }
}
