use archiveos_contract::{ImportManifest, JobProgress, JobProgressStep, ManifestItem, VaultError, YtdlpJobInput};
use crate::failures::SourceFailure;
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
    let _ = purge_terminal_jobs(conn, 24);
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

const TERMINAL_JOB_STATUSES: &[&str] = &["done", "partial", "failed", "cancelled"];

pub fn is_terminal_job_status(status: &str) -> bool {
    TERMINAL_JOB_STATUSES.contains(&status)
}

pub fn delete_job(conn: &Connection, job_id: Uuid) -> Result<(), VaultError> {
    let job = get_job(conn, job_id)?.ok_or(VaultError::NotFound)?;
    if !is_terminal_job_status(&job.status) {
        return Err(VaultError::InvalidLayout {
            detail: format!("job {} is not terminal (status={})", job_id, job.status),
        });
    }
    conn.execute(
        "DELETE FROM job WHERE parent_job_id = ?1",
        [job_id.to_string()],
    )
    .map_err(db_err)?;
    conn.execute(
        "DELETE FROM source_failure WHERE job_id = ?1",
        [job_id.to_string()],
    )
    .map_err(db_err)?;
    let changed = conn
        .execute("DELETE FROM job WHERE id = ?1", [job_id.to_string()])
        .map_err(db_err)?;
    if changed == 0 {
        return Err(VaultError::NotFound);
    }
    Ok(())
}

/// Remove successful terminal jobs older than `ttl_hours`.
pub fn purge_terminal_jobs(conn: &Connection, ttl_hours: i64) -> Result<u32, VaultError> {
    let cutoff = (Utc::now() - Duration::hours(ttl_hours)).to_rfc3339();
    let changed = conn
        .execute(
            "DELETE FROM job
             WHERE status IN ('done', 'cancelled')
               AND created_at < ?1",
            [cutoff],
        )
        .map_err(db_err)?;
    Ok(changed as u32)
}

fn manifest_item_label(item: &archiveos_contract::ManifestItem) -> String {
    if let Some(meta) = item.metadata.as_ref() {
        if let Some(title) = meta.get("title").and_then(|v| v.as_str()) {
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }
    if let Some(ytdlp) = item.metadata_by_provenance.get("yt-dlp") {
        if let Some(title) = ytdlp.get("title").and_then(|v| v.as_str()) {
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }
    item.source_ref
        .as_ref()
        .map(|r| r.external_id.clone())
        .unwrap_or_else(|| item.path.clone())
}

const LIFECYCLE_ITEM_STATUSES: &[&str] = &["unavailable", "private", "region_locked", "dead"];

fn is_lifecycle_item_status(status: &str) -> bool {
    LIFECYCLE_ITEM_STATUSES.contains(&status)
}

fn manifest_item_step_status(status: &str) -> &'static str {
    match status {
        "complete" => "done",
        "discovered" => "skipped",
        s if is_lifecycle_item_status(s) => "unavailable",
        "failed" => "failed",
        _ => "failed",
    }
}

pub fn resolve_manifest_job_status(manifest: &ImportManifest, explicit: Option<&str>) -> String {
    if let Some(status) = explicit {
        return match status {
            "done" | "partial" | "failed" => status.to_string(),
            _ => "done".to_string(),
        };
    }

    let video_items: Vec<_> = manifest
        .items
        .iter()
        .filter(|item| item.source_ref.as_ref().is_some_and(|r| r.kind == "video"))
        .collect();
    if video_items.is_empty() {
        return "done".to_string();
    }

    let complete = video_items
        .iter()
        .filter(|item| item.status == "complete")
        .count();
    let hard_failed = video_items
        .iter()
        .filter(|item| {
            !matches!(
                item.status.as_str(),
                "complete" | "discovered"
            ) && !is_lifecycle_item_status(&item.status)
        })
        .count();
    let lifecycle = video_items
        .iter()
        .filter(|item| is_lifecycle_item_status(&item.status))
        .count();

    if hard_failed > 0 && complete > 0 {
        "partial".to_string()
    } else if hard_failed > 0 {
        "failed".to_string()
    } else if lifecycle > 0 {
        "partial".to_string()
    } else {
        "done".to_string()
    }
}

fn finish_progress_summary_label(video_items: &[&ManifestItem]) -> Option<String> {
    if video_items.is_empty() {
        return None;
    }
    let complete = video_items
        .iter()
        .filter(|item| item.status == "complete")
        .count();
    let discovered = video_items
        .iter()
        .filter(|item| item.status == "discovered")
        .count();
    let lifecycle = video_items
        .iter()
        .filter(|item| is_lifecycle_item_status(&item.status))
        .count();
    let hard_failed = video_items
        .iter()
        .filter(|item| {
            !matches!(
                item.status.as_str(),
                "complete" | "discovered"
            ) && !is_lifecycle_item_status(&item.status)
        })
        .count();

    let mut parts = Vec::new();
    if complete > 0 {
        parts.push(format!("{complete} OK"));
    }
    if discovered > 0 {
        parts.push(format!("{discovered} skipped"));
    }
    if lifecycle > 0 {
        parts.push(format!("{lifecycle} unavailable"));
    }
    if hard_failed > 0 {
        parts.push(format!("{hard_failed} failed"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

fn failure_message_for_item<'a>(
    failures: &'a [SourceFailure],
    external_id: &str,
    kind: &str,
) -> Option<&'a str> {
    failures
        .iter()
        .find(|f| f.external_id == external_id && f.kind == kind)
        .map(|f| f.message.as_str())
}

pub fn build_finish_progress(manifest: &ImportManifest, failures: &[SourceFailure]) -> JobProgress {
    use std::collections::HashMap;

    let mut steps_by_id: HashMap<String, JobProgressStep> = HashMap::new();
    for item in &manifest.items {
        let Some(source_ref) = item.source_ref.as_ref() else {
            continue;
        };
        if !matches!(source_ref.kind.as_str(), "video" | "thumbnail" | "avatar") {
            continue;
        }
        let label = match source_ref.kind.as_str() {
            "thumbnail" => format!("Thumbnail: {}", manifest_item_label(item)),
            "avatar" => format!("Avatar: {}", manifest_item_label(item)),
            _ => manifest_item_label(item),
        };
        let message = failure_message_for_item(failures, &source_ref.external_id, &source_ref.kind)
            .map(str::to_string);
        steps_by_id.insert(
            source_ref.external_id.clone(),
            JobProgressStep {
                id: source_ref.external_id.clone(),
                label,
                status: manifest_item_step_status(&item.status).to_string(),
                percent: None,
                message,
            },
        );
    }

    if !manifest.membership.is_empty() {
        for member in &manifest.membership {
            if member.kind != "video" {
                continue;
            }
            if steps_by_id.contains_key(&member.external_id) {
                continue;
            }
            let message = failure_message_for_item(failures, &member.external_id, "video")
                .map(str::to_string);
            steps_by_id.insert(
                member.external_id.clone(),
                JobProgressStep {
                    id: member.external_id.clone(),
                    label: member.external_id.clone(),
                    status: "skipped".to_string(),
                    percent: None,
                    message,
                },
            );
        }
    }

    if steps_by_id.is_empty() {
        for failure in failures {
            let label = match failure.kind.as_str() {
                "thumbnail" => format!("Thumbnail {}", failure.external_id),
                "avatar" => format!("Avatar {}", failure.external_id),
                "video" => failure.external_id.clone(),
                _ => format!("{}/{}", failure.kind, failure.external_id),
            };
            steps_by_id.insert(
                failure.external_id.clone(),
                JobProgressStep {
                    id: failure.external_id.clone(),
                    label,
                    status: "failed".to_string(),
                    percent: None,
                    message: Some(failure.message.clone()),
                },
            );
        }
    }

    let steps: Vec<JobProgressStep> = if manifest.membership.is_empty() {
        let mut ordered = Vec::new();
        for item in &manifest.items {
            let Some(source_ref) = item.source_ref.as_ref() else {
                continue;
            };
            if let Some(step) = steps_by_id.remove(&source_ref.external_id) {
                ordered.push(step);
            }
        }
        ordered.extend(steps_by_id.into_values());
        ordered
    } else {
        let mut ordered = Vec::new();
        for member in &manifest.membership {
            if member.kind != "video" {
                continue;
            }
            if let Some(step) = steps_by_id.remove(&member.external_id) {
                ordered.push(step);
            }
        }
        ordered.extend(steps_by_id.into_values());
        ordered
    };

    let video_items: Vec<_> = manifest
        .items
        .iter()
        .filter(|i| i.source_ref.as_ref().is_some_and(|r| r.kind == "video"))
        .collect();
    let complete = video_items
        .iter()
        .filter(|i| i.status == "complete")
        .count();
    let total = if !manifest.membership.is_empty() {
        manifest
            .membership
            .iter()
            .filter(|m| m.kind == "video")
            .count()
    } else {
        video_items.len()
    };
    let label = finish_progress_summary_label(&video_items).or_else(|| {
        if !steps.is_empty() {
            Some(format!(
                "{} issue(s)",
                steps.iter().filter(|s| s.status == "failed").count()
            ))
        } else {
            None
        }
    });

    JobProgress {
        phase: "finished".into(),
        current: Some(complete as i32),
        total: if total > 0 {
            Some(total as i32)
        } else {
            Some(steps.len() as i32)
        },
        label,
        percent: None,
        steps,
    }
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
                message: None,
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

    #[test]
    fn delete_terminal_job_cascades_children() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let parent = create_job(conn, "yt-dlp", "test", "url").unwrap();
        finish_job(conn, parent.id, "failed").unwrap();
        let child = create_job_with_options(
            conn,
            CreateJobOptions {
                job_type: "preview",
                target_vault: "test",
                input: "{}",
                parent_job_id: Some(parent.id),
            },
        )
        .unwrap();
        finish_job(conn, child.id, "done").unwrap();

        delete_job(conn, parent.id).unwrap();
        assert!(get_job(conn, parent.id).unwrap().is_none());
        assert!(get_job(conn, child.id).unwrap().is_none());
    }

    #[test]
    fn delete_rejects_running_job() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let job = create_job(conn, "yt-dlp", "test", "url").unwrap();
        claim_job(conn, 60, None).unwrap();
        assert!(delete_job(conn, job.id).is_err());
    }

    #[test]
    fn purge_removes_old_done_jobs() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();

        let job = create_job(conn, "yt-dlp", "test", "url").unwrap();
        finish_job(conn, job.id, "done").unwrap();
        let old = (Utc::now() - Duration::hours(48)).to_rfc3339();
        conn.execute(
            "UPDATE job SET created_at = ?1 WHERE id = ?2",
            params![old, job.id.to_string()],
        )
        .unwrap();

        let purged = purge_terminal_jobs(conn, 24).unwrap();
        assert_eq!(purged, 1);
        assert!(get_job(conn, job.id).unwrap().is_none());
    }

    #[test]
    fn build_finish_progress_from_manifest_and_failures() {
        use archiveos_contract::{
            ImportManifest, ImportStrategy, ManifestItem, ManifestSourceRef,
        };
        use crate::failures::{RecordFailureInput, record_failure};

        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let job = create_job(conn, "yt-dlp", "test", "url").unwrap();

        record_failure(
            conn,
            &RecordFailureInput {
                job_id: Some(job.id),
                source: "youtube".into(),
                kind: "video".into(),
                external_id: "vid2".into(),
                url: None,
                stage: "download".into(),
                error_kind: "unavailable".into(),
                message: "video unavailable".into(),
                retryable: false,
            },
        )
        .unwrap();

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: Some("youtube".into()),
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![
                ManifestItem {
                    path: "files/vid1.mp4".into(),
                    sha256: None,
                    status: "complete".into(),
                    source_ref: Some(ManifestSourceRef {
                        source: "youtube".into(),
                        kind: "video".into(),
                        external_id: "vid1".into(),
                        url: None,
                    }),
                    metadata: Some(serde_json::json!({ "title": "Good video" })),
                    metadata_by_provenance: Default::default(),
                    assets: vec![],
                },
                ManifestItem {
                    path: "".into(),
                    sha256: None,
                    status: "unavailable".into(),
                    source_ref: Some(ManifestSourceRef {
                        source: "youtube".into(),
                        kind: "video".into(),
                        external_id: "vid2".into(),
                        url: None,
                    }),
                    metadata: Some(serde_json::json!({ "title": "Gone video" })),
                    metadata_by_provenance: Default::default(),
                    assets: vec![],
                },
            ],
            membership: vec![],
            relations: vec![],
        };

        let failures = crate::failures::list_failures_for_job(conn, job.id).unwrap();
        let progress = build_finish_progress(&manifest, &failures);
        assert_eq!(progress.phase, "finished");
        assert_eq!(progress.steps.len(), 2);
        assert_eq!(progress.steps[0].status, "done");
        assert_eq!(progress.steps[1].status, "unavailable");
        assert_eq!(
            progress.steps[1].message.as_deref(),
            Some("video unavailable")
        );
        assert_eq!(progress.label.as_deref(), Some("1 OK, 1 unavailable"));
    }

    #[test]
    fn resolve_manifest_job_status_treats_lifecycle_as_done() {
        use archiveos_contract::{
            ImportManifest, ImportStrategy, ManifestItem, ManifestSourceRef,
        };

        let manifest = ImportManifest {
            source: "yt-dlp".into(),
            source_identity: Some("youtube".into()),
            vault: "test".into(),
            strategy: ImportStrategy::Managed,
            collection: None,
            channels: vec![],
            items: vec![
                ManifestItem {
                    path: "".into(),
                    sha256: None,
                    status: "discovered".into(),
                    source_ref: Some(ManifestSourceRef {
                        source: "youtube".into(),
                        kind: "video".into(),
                        external_id: "vid1".into(),
                        url: None,
                    }),
                    metadata: None,
                    metadata_by_provenance: Default::default(),
                    assets: vec![],
                },
                ManifestItem {
                    path: "".into(),
                    sha256: None,
                    status: "unavailable".into(),
                    source_ref: Some(ManifestSourceRef {
                        source: "youtube".into(),
                        kind: "video".into(),
                        external_id: "vid2".into(),
                        url: None,
                    }),
                    metadata: None,
                    metadata_by_provenance: Default::default(),
                    assets: vec![],
                },
            ],
            membership: vec![],
            relations: vec![],
        };

        assert_eq!(
            resolve_manifest_job_status(&manifest, None),
            "partial".to_string()
        );
    }
}
