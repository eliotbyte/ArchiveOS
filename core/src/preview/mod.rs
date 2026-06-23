use std::path::Path;

use archiveos_contract::VaultError;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::assets::{self, UpsertAssetInput};
use crate::jobs;

pub const JOB_TYPE_PREVIEW: &str = "preview";

pub const ROLE_PREVIEW_IMAGE_SMALL: &str = "preview_image_small";
pub const ROLE_PREVIEW_IMAGE_LARGE: &str = "preview_image_large";
pub const ROLE_TIMELINE_SPRITE: &str = "timeline_sprite";
pub const ROLE_TIMELINE_MANIFEST: &str = "timeline_manifest";

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PreviewJobInput {
    pub entity_id: Uuid,
    #[serde(default)]
    pub parent_job_id: Option<Uuid>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PreviewFileCommit {
    pub path: String,
    pub preview_role: String,
    pub kind: String,
    pub mime: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PreviewBackfillReport {
    pub scanned: u32,
    pub queued: u32,
    pub skipped: u32,
}

pub fn backfill_preview_jobs(
    conn: &Connection,
    target_vault: &str,
) -> Result<PreviewBackfillReport, VaultError> {
    let mut report = PreviewBackfillReport::default();
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT ea.entity_id
             FROM entity_asset ea
             JOIN entity e ON e.id = ea.entity_id
             WHERE ea.role = 'primary'
               AND ea.status = 'present'
               AND ea.mime IS NOT NULL
               AND (
                 ea.mime LIKE 'image/%' OR ea.mime LIKE 'video/%'
               )
               AND (
                 ea.path IS NOT NULL OR ea.content_hash IS NOT NULL
               )
               AND e.status != 'user_deleted'",
        )
        .map_err(db_err)?;

    let entity_ids = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;

    for id_str in entity_ids {
        report.scanned += 1;
        let entity_id = Uuid::parse_str(&id_str).map_err(|e| VaultError::InvalidLayout {
            detail: e.to_string(),
        })?;
        match maybe_enqueue_preview_job(conn, target_vault, entity_id, None)? {
            Some(_) => report.queued += 1,
            None => report.skipped += 1,
        }
    }

    Ok(report)
}

pub fn maybe_enqueue_preview_job(
    conn: &Connection,
    target_vault: &str,
    entity_id: Uuid,
    parent_job_id: Option<Uuid>,
) -> Result<Option<jobs::Job>, VaultError> {
    if assets::find_asset_by_preview_role(conn, entity_id, ROLE_PREVIEW_IMAGE_SMALL)?.is_some() {
        return Ok(None);
    }

    let primary = assets::find_primary_asset(conn, entity_id)?;
    let Some(primary) = primary else {
        return Ok(None);
    };
    if primary.status != "present" {
        return Ok(None);
    }
    if primary.path.is_none() && primary.content_hash.is_none() {
        return Ok(None);
    }

    let mime = primary.mime.as_deref().unwrap_or("");
    if !mime.starts_with("image/") && !mime.starts_with("video/") {
        return Ok(None);
    }

    if pending_preview_job_for_entity(conn, entity_id)? {
        return Ok(None);
    }

    let mut input = serde_json::json!({ "entity_id": entity_id });
    if let Some(parent) = parent_job_id {
        input["parent_job_id"] = serde_json::Value::String(parent.to_string());
    }
    jobs::create_job_with_options(
        conn,
        jobs::CreateJobOptions {
            job_type: JOB_TYPE_PREVIEW,
            target_vault,
            input: &input.to_string(),
            parent_job_id,
        },
    )
    .map(Some)
}

fn pending_preview_job_for_entity(
    conn: &Connection,
    entity_id: Uuid,
) -> Result<bool, VaultError> {
    let needle = entity_id.to_string();
    let count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM job
             WHERE type = ?1 AND status IN ('pending', 'claimed', 'running')
               AND input LIKE '%' || ?2 || '%'",
            params![JOB_TYPE_PREVIEW, needle],
            |row| row.get(0),
        )
        .map_err(db_err)?;
    Ok(count > 0)
}

pub fn commit_preview_files(
    vault: &crate::Vault,
    job_id: Uuid,
    entity_id: Uuid,
    files: &[PreviewFileCommit],
) -> Result<(), VaultError> {
    let staging = vault.staging_job_dir(&job_id.to_string());
    for file in files {
        let staging_file = resolve_staging_file(&staging, &file.path)?;
        let ext = extension_from_path(&staging_file);
        let ext_for_store = ext.strip_prefix('.').unwrap_or(&ext);
        let cas = vault.store_managed(&staging_file, ext_for_store)?;
        let path_str = cas.blob_path.to_string_lossy().into_owned();

        let asset_id = assets::upsert_preview_asset(
            vault.connection(),
            entity_id,
            &file.preview_role,
            &file.kind,
            &UpsertAssetInput {
                entity_id,
                role: "supporting",
                kind: &file.kind,
                content_hash: Some(&cas.content_hash),
                mime: Some(&file.mime),
                size: cas.size,
                ext: Some(&ext),
                status: "present",
                storage_strategy: "managed",
                path: Some(&path_str),
            },
        )?;
        assets::upsert_asset_metadata(
            vault.connection(),
            asset_id,
            "preview_role",
            &file.preview_role,
            "archiveos",
        )?;
    }
    Ok(())
}

fn resolve_staging_file(staging: &Path, relative_path: &str) -> Result<std::path::PathBuf, VaultError> {
    let normalized = relative_path.trim_start_matches('/');
    let candidate = staging.join("files").join(normalized);
    if candidate.is_file() {
        return Ok(candidate);
    }
    let direct = staging.join(normalized);
    if direct.is_file() {
        return Ok(direct);
    }
    Err(VaultError::InvalidLayout {
        detail: format!("staging file not found: {relative_path}"),
    })
}

fn extension_from_path(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default()
}

fn db_err(err: rusqlite::Error) -> VaultError {
    VaultError::Database {
        detail: err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::db::insert_entity;
    use crate::Vault;
    use chrono::Utc;
    use tempfile::tempdir;

    #[test]
    fn enqueue_preview_job_for_entity_with_present_primary() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(
            vault.connection(),
            entity_id,
            Some("hash"),
            Some("video/mp4"),
            10,
            "present",
            &now,
            None,
        )
        .unwrap();
        assets::upsert_asset(
            vault.connection(),
            &UpsertAssetInput {
                entity_id,
                role: "primary",
                kind: "video",
                content_hash: Some("hash"),
                mime: Some("video/mp4"),
                size: 10,
                ext: Some(".mp4"),
                status: "present",
                storage_strategy: "managed",
                path: Some("/tmp/video.mp4"),
            },
        )
        .unwrap();

        let job = maybe_enqueue_preview_job(vault.connection(), "vault", entity_id, None)
            .unwrap()
            .expect("job");
        assert_eq!(job.job_type, JOB_TYPE_PREVIEW);
    }

    #[test]
    fn backfill_queues_jobs_for_image_entities_without_previews() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(
            vault.connection(),
            entity_id,
            Some("img"),
            Some("image/jpeg"),
            10,
            "present",
            &now,
            None,
        )
        .unwrap();
        assets::upsert_asset(
            vault.connection(),
            &UpsertAssetInput {
                entity_id,
                role: "primary",
                kind: "image",
                content_hash: Some("img"),
                mime: Some("image/jpeg"),
                size: 10,
                ext: Some(".jpg"),
                status: "present",
                storage_strategy: "managed",
                path: Some("/tmp/photo.jpg"),
            },
        )
        .unwrap();

        let report = backfill_preview_jobs(vault.connection(), "vault").unwrap();
        assert_eq!(report.scanned, 1);
        assert_eq!(report.queued, 1);
    }

    #[test]
    fn backfill_queues_jobs_for_managed_assets_without_path() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        insert_entity(
            vault.connection(),
            entity_id,
            Some("hash"),
            Some("video/mp4"),
            10,
            "present",
            &now,
            None,
        )
        .unwrap();
        assets::upsert_asset(
            vault.connection(),
            &UpsertAssetInput {
                entity_id,
                role: "primary",
                kind: "video",
                content_hash: Some("hash"),
                mime: Some("video/mp4"),
                size: 10,
                ext: Some(".mp4"),
                status: "present",
                storage_strategy: "managed",
                path: None,
            },
        )
        .unwrap();

        let report = backfill_preview_jobs(vault.connection(), "vault").unwrap();
        assert_eq!(report.scanned, 1);
        assert_eq!(report.queued, 1);
    }
}
