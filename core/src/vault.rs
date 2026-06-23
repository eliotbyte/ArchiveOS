use std::fs;
use std::path::{Path, PathBuf};

use archiveos_contract::{
    ImportManifest, ImportReport, ImportStrategy, VaultError, VaultMetadata,
    YtdlpJobInput, ARCHIVEOS_DIR, BLOBS_DIR, DB_FILE, INBOX_DIR, STAGING_DIR, VAULT_JSON,
};
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::cas::{self, CasStoreResult};
use crate::db::{migrate, open_connection};
use crate::layout::{
    blob_path as layout_blob_path, find_blob_path, is_dir_empty, staging_job_path,
    thumbnail_worker_dir, validate_layout, worker_cache_dir, worker_cookies_dir, ytdlp_worker_dir,
};

pub struct Vault {
    root: PathBuf,
    metadata: VaultMetadata,
    db: Connection,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VaultCapability {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

impl Vault {
    pub fn init(root: impl AsRef<Path>) -> Result<Self, VaultError> {
        let root = root.as_ref().to_path_buf();

        if root.exists() {
            if !is_dir_empty(&root)? {
                return Err(VaultError::AlreadyExists);
            }
        } else {
            fs::create_dir_all(&root)?;
        }

        let archiveos = root.join(ARCHIVEOS_DIR);
        fs::create_dir_all(&archiveos)?;

        let metadata = VaultMetadata::new(Uuid::new_v4(), Utc::now());
        let vault_json = archiveos.join(VAULT_JSON);
        fs::write(&vault_json, serde_json::to_string_pretty(&metadata)?)?;

        fs::write(archiveos.join(DB_FILE), [])?;
        fs::create_dir_all(root.join(BLOBS_DIR))?;
        fs::create_dir_all(root.join(STAGING_DIR))?;
        fs::create_dir_all(root.join(INBOX_DIR))?;

        let db = open_connection(&archiveos.join(DB_FILE))?;
        migrate(&db)?;

        Ok(Self { root, metadata, db })
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, VaultError> {
        let root = root.as_ref().to_path_buf();
        let metadata = validate_layout(&root)?;
        crate::inbox::ensure_inbox_dir(&root)?;
        let db = open_connection(&root.join(ARCHIVEOS_DIR).join(DB_FILE))?;
        migrate(&db)?;
        Ok(Self { root, metadata, db })
    }

    /// Open an existing vault or create layout under `root` (allows non-empty host dirs).
    pub fn ensure(root: impl AsRef<Path>) -> Result<Self, VaultError> {
        let root = root.as_ref();
        if let Ok(vault) = Self::open(root) {
            return Ok(vault);
        }
        if !root.exists() || is_dir_empty(root)? {
            return Self::init(root);
        }
        Self::bootstrap_layout(root)
    }

    fn bootstrap_layout(root: impl AsRef<Path>) -> Result<Self, VaultError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;

        let archiveos = root.join(ARCHIVEOS_DIR);
        fs::create_dir_all(&archiveos)?;
        fs::create_dir_all(root.join(BLOBS_DIR))?;
        fs::create_dir_all(root.join(STAGING_DIR))?;
        fs::create_dir_all(root.join(INBOX_DIR))?;

        let vault_json = archiveos.join(VAULT_JSON);
        let metadata = if vault_json.is_file() {
            let contents = fs::read_to_string(&vault_json)?;
            serde_json::from_str(&contents)?
        } else {
            let metadata = VaultMetadata::new(Uuid::new_v4(), Utc::now());
            fs::write(&vault_json, serde_json::to_string_pretty(&metadata)?)?;
            metadata
        };

        let db_path = archiveos.join(DB_FILE);
        if !db_path.is_file() {
            fs::write(&db_path, [])?;
        }

        let db = open_connection(&db_path)?;
        migrate(&db)?;

        Ok(Self { root, metadata, db })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn id(&self) -> Uuid {
        self.metadata.id
    }

    pub fn metadata(&self) -> &VaultMetadata {
        &self.metadata
    }

    pub fn connection(&self) -> &Connection {
        &self.db
    }

    pub fn archiveos_dir(&self) -> PathBuf {
        self.root.join(ARCHIVEOS_DIR)
    }

    pub fn blobs_dir(&self) -> PathBuf {
        self.root.join(BLOBS_DIR)
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.root.join(STAGING_DIR)
    }

    pub fn inbox_dir(&self) -> PathBuf {
        self.root.join(INBOX_DIR)
    }

    pub fn db_path(&self) -> PathBuf {
        self.archiveos_dir().join(DB_FILE)
    }

    pub fn blob_path(&self, hash: &str, ext: &str) -> Result<PathBuf, VaultError> {
        layout_blob_path(&self.root, hash, ext)
    }

    pub fn staging_job_dir(&self, job_id: &str) -> PathBuf {
        staging_job_path(&self.root, job_id)
    }

    pub fn store_managed(
        &self,
        staging_file: &Path,
        ext: &str,
    ) -> Result<CasStoreResult, VaultError> {
        cas::store_managed(&self.root, staging_file, ext)
    }

    pub fn blob_exists(&self, hash: &str, ext: &str) -> Result<bool, VaultError> {
        cas::blob_exists(&self.root, hash, ext)
    }

    pub fn import(
        &self,
        staging_dir: &Path,
        manifest: Option<&ImportManifest>,
        strategy: ImportStrategy,
    ) -> Result<ImportReport, VaultError> {
        crate::import::import(self, staging_dir, manifest, strategy)
    }

    pub fn import_chunk(
        &self,
        staging_dir: &Path,
        manifest: &ImportManifest,
        strategy: ImportStrategy,
        finalize: bool,
    ) -> Result<ImportReport, VaultError> {
        crate::import::import_with_options(
            self,
            staging_dir,
            Some(manifest),
            strategy,
            crate::import::ImportOptions {
                cleanup_staging: finalize,
                finalize_membership: finalize,
            },
        )
    }

    pub fn process_inbox(&self, target_vault: Option<&str>) -> Result<crate::inbox::InboxReport, VaultError> {
        crate::inbox::process_inbox(self, target_vault)
    }

    pub fn maybe_enqueue_preview_job(
        &self,
        target_vault: &str,
        entity_id: Uuid,
        parent_job_id: Option<Uuid>,
    ) -> Result<Option<crate::jobs::Job>, VaultError> {
        crate::preview::maybe_enqueue_preview_job(
            self.connection(),
            target_vault,
            entity_id,
            parent_job_id,
        )
    }

    pub fn backfill_preview_jobs(
        &self,
        target_vault: &str,
    ) -> Result<crate::preview::PreviewBackfillReport, VaultError> {
        crate::preview::backfill_preview_jobs(self.connection(), target_vault)
    }

    pub fn regenerate_preview_job(
        &self,
        target_vault: &str,
        entity_id: Uuid,
    ) -> Result<crate::jobs::Job, VaultError> {
        let input = serde_json::json!({ "entity_id": entity_id });
        crate::jobs::create_job(
            self.connection(),
            crate::preview::JOB_TYPE_PREVIEW,
            target_vault,
            &input.to_string(),
        )
    }

    pub fn commit_preview_files(
        &self,
        job_id: Uuid,
        entity_id: Uuid,
        files: &[crate::preview::PreviewFileCommit],
    ) -> Result<(), VaultError> {
        crate::preview::commit_preview_files(self, job_id, entity_id, files)
    }

    pub fn list_collections(
        &self,
        query: &crate::collections::CollectionQuery,
    ) -> Result<Vec<crate::collections::CollectionSummary>, VaultError> {
        crate::collections::list_collections(self.connection(), query)
    }

    pub fn get_collection(
        &self,
        collection_id: Uuid,
    ) -> Result<crate::collections::CollectionDetail, VaultError> {
        crate::collections::get_collection(self.connection(), Some(&self.root), collection_id)
    }

    pub fn list_collection_members(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<crate::collections::CollectionMemberItem>, VaultError> {
        crate::collections::list_collection_members(self.connection(), Some(&self.root), collection_id)
    }

    pub fn list_library_lists(
        &self,
        query: &archiveos_contract::LibraryListQuery,
    ) -> Result<Vec<archiveos_contract::LibraryListSummary>, VaultError> {
        crate::library_lists::list_library_lists(self.connection(), query)
    }

    pub fn get_library_list(
        &self,
        list_id: &str,
        sort: Option<&str>,
        scope: Option<&archiveos_contract::LibraryListQuery>,
    ) -> Result<archiveos_contract::LibraryListDetail, VaultError> {
        crate::library_lists::get_library_list(self.connection(), Some(&self.root), list_id, sort, scope)
    }

    pub fn upsert_playback_state(
        &self,
        entity_id: Uuid,
        input: &archiveos_contract::PlaybackStateInput,
    ) -> Result<archiveos_contract::PlaybackStateResponse, VaultError> {
        let user_id = crate::playback::ensure_default_user(self.connection())?;
        crate::playback::upsert_playback_state(self.connection(), user_id, entity_id, input)
    }

    pub fn get_playback_state(
        &self,
        entity_id: Uuid,
    ) -> Result<Option<archiveos_contract::PlaybackStateResponse>, VaultError> {
        let user_id = crate::playback::ensure_default_user(self.connection())?;
        crate::playback::get_playback_state(self.connection(), user_id, entity_id)
    }

    pub fn dismiss_playback_state(&self, entity_id: Uuid) -> Result<(), VaultError> {
        let user_id = crate::playback::ensure_default_user(self.connection())?;
        crate::playback::dismiss_playback_state(self.connection(), user_id, entity_id)
    }

    pub fn create_user_list(
        &self,
        input: &archiveos_contract::CreateUserListInput,
    ) -> Result<Uuid, VaultError> {
        let user_id = crate::playback::ensure_default_user(self.connection())?;
        crate::user_lists::create_user_list(self.connection(), user_id, input)
    }

    pub fn add_user_list_member(
        &self,
        list_id: Uuid,
        input: &archiveos_contract::AddUserListMemberInput,
    ) -> Result<(), VaultError> {
        crate::user_lists::add_user_list_member(self.connection(), list_id, input)
    }

    pub fn remove_user_list_member(
        &self,
        list_id: Uuid,
        entity_id: Uuid,
    ) -> Result<(), VaultError> {
        crate::user_lists::remove_user_list_member(self.connection(), list_id, entity_id)
    }

    pub fn reorder_user_list_members(
        &self,
        list_id: Uuid,
        entity_ids: &[Uuid],
    ) -> Result<(), VaultError> {
        crate::user_lists::reorder_user_list_members(self.connection(), list_id, entity_ids)
    }

    pub fn ensure_watch_later_list(&self) -> Result<Uuid, VaultError> {
        let user_id = crate::playback::ensure_default_user(self.connection())?;
        crate::user_lists::ensure_watch_later_list(self.connection(), user_id)
    }

    pub fn vault_capabilities(&self) -> Vec<VaultCapability> {
        vec![
            VaultCapability {
                id: "ytdlp".into(),
                label: "yt-dlp".into(),
                enabled: self.ytdlp_worker_dir().exists(),
            },
            VaultCapability {
                id: "thumbnail".into(),
                label: "Preview generator".into(),
                enabled: self.thumbnail_worker_dir().exists(),
            },
        ]
    }

    pub fn add_tag(&self, entity_id: Uuid, tag: &str) -> Result<(), VaultError> {
        crate::tags::add_tag(self.connection(), entity_id, tag)
    }

    pub fn remove_tag(&self, entity_id: Uuid, tag: &str) -> Result<(), VaultError> {
        crate::tags::remove_tag(self.connection(), entity_id, tag)
    }

    pub fn list_tags(&self, entity_id: Uuid) -> Result<Vec<String>, VaultError> {
        crate::tags::list_entity_tags(self.connection(), entity_id)
    }

    pub fn search(&self, query: &archiveos_contract::SearchQuery) -> Result<Vec<archiveos_contract::EntityHit>, VaultError> {
        crate::search::search(self.connection(), query)
    }

    pub fn browse(
        &self,
        query: &archiveos_contract::BrowseQuery,
    ) -> Result<Vec<archiveos_contract::EntityListItem>, VaultError> {
        crate::browse::browse_at(self.connection(), Some(&self.root), query)
    }

    pub fn get_channel(
        &self,
        channel_entity_id: uuid::Uuid,
    ) -> Result<archiveos_contract::ChannelDetail, VaultError> {
        crate::channels::get_channel_detail(self.connection(), channel_entity_id)
    }

    pub fn list_channel_videos(
        &self,
        channel_entity_id: uuid::Uuid,
        limit: u32,
        sort: Option<&str>,
    ) -> Result<Vec<archiveos_contract::EntityListItem>, VaultError> {
        let _ = crate::channels::get_channel_detail(self.connection(), channel_entity_id)?;
        let sort = sort
            .and_then(archiveos_contract::BrowseSort::parse)
            .unwrap_or(archiveos_contract::BrowseSort::PublishedDesc);
        crate::browse::browse_sorted_at(
            self.connection(),
            Some(&self.root),
            &archiveos_contract::BrowseQuery {
                uploaded_by: Some(channel_entity_id),
                limit,
                sort: Some(sort.as_str().to_string()),
                ..Default::default()
            },
            sort,
        )
    }

    pub fn resolve_asset_content_path(
        &self,
        asset_id: Uuid,
    ) -> Result<(PathBuf, String), VaultError> {
        let asset = crate::assets::get_asset(self.connection(), asset_id)?;
        if asset.status != "present" {
            return Err(VaultError::InvalidLayout {
                detail: format!(
                    "asset {asset_id} is not present (status={})",
                    asset.status
                ),
            });
        }

        let mime = asset
            .mime
            .clone()
            .unwrap_or_else(|| "application/octet-stream".into());

        match asset.storage_strategy.as_str() {
            "managed" => {
                let hash = asset.content_hash.ok_or_else(|| VaultError::InvalidLayout {
                    detail: format!("managed asset {asset_id} missing content_hash"),
                })?;
                let path = find_blob_path(
                    &self.root,
                    &hash,
                    asset.ext.as_deref(),
                    Some(mime.as_str()),
                )?;
                Ok((path, mime))
            }
            "reference" => {
                let path_str = asset.path.ok_or_else(|| VaultError::InvalidLayout {
                    detail: format!("reference asset {asset_id} missing path"),
                })?;
                let path = PathBuf::from(&path_str);
                if !path.is_file() {
                    return Err(VaultError::NotFound);
                }
                Ok((path, mime))
            }
            other => Err(VaultError::InvalidLayout {
                detail: format!("asset {asset_id} storage_strategy '{other}' is not readable"),
            }),
        }
    }

    pub fn ytdlp_worker_dir(&self) -> PathBuf {
        ytdlp_worker_dir(&self.root)
    }

    pub fn ytdlp_cache_dir(&self) -> PathBuf {
        worker_cache_dir(&self.root, archiveos_contract::WORKER_YTDLP)
    }

    pub fn ytdlp_cookies_dir(&self) -> PathBuf {
        worker_cookies_dir(&self.root, archiveos_contract::WORKER_YTDLP)
    }

    pub fn thumbnail_worker_dir(&self) -> PathBuf {
        thumbnail_worker_dir(&self.root)
    }

    pub fn get_entity(&self, entity_id: Uuid) -> Result<crate::entity::EntityDetail, VaultError> {
        crate::entity::get_entity_at(self.connection(), entity_id, Some(&self.root))
    }

    pub fn create_job(&self, job_type: &str, target_vault: &str, input: &str) -> Result<crate::jobs::Job, VaultError> {
        crate::jobs::create_job(self.connection(), job_type, target_vault, input)
    }

    pub fn create_job_with_parent(
        &self,
        job_type: &str,
        target_vault: &str,
        input: &str,
        parent_job_id: Option<Uuid>,
    ) -> Result<crate::jobs::Job, VaultError> {
        crate::jobs::create_job_with_options(
            self.connection(),
            crate::jobs::CreateJobOptions {
                job_type,
                target_vault,
                input,
                parent_job_id,
            },
        )
    }

    pub fn claim_job(
        &self,
        lease_secs: i64,
        job_type: Option<&str>,
    ) -> Result<Option<crate::jobs::Job>, VaultError> {
        crate::jobs::claim_job(self.connection(), lease_secs, job_type)
    }

    pub fn heartbeat_job(
        &self,
        job_id: Uuid,
        lease_secs: i64,
        progress: Option<&archiveos_contract::JobProgress>,
    ) -> Result<bool, VaultError> {
        crate::jobs::heartbeat_job(self.connection(), job_id, lease_secs, progress)
    }

    pub fn cancel_job(&self, job_id: Uuid) -> Result<crate::jobs::Job, VaultError> {
        crate::jobs::cancel_job(self.connection(), job_id)
    }

    pub fn finish_job(&self, job_id: Uuid, status: &str) -> Result<(), VaultError> {
        crate::jobs::finish_job(self.connection(), job_id, status)
    }

    pub fn delete_job(&self, job_id: Uuid) -> Result<(), VaultError> {
        crate::jobs::delete_job(self.connection(), job_id)
    }

    pub fn update_job_progress(
        &self,
        job_id: Uuid,
        progress: &archiveos_contract::JobProgress,
    ) -> Result<(), VaultError> {
        crate::jobs::update_job_progress(self.connection(), job_id, progress)
    }

    pub fn get_job(&self, job_id: Uuid) -> Result<Option<crate::jobs::Job>, VaultError> {
        crate::jobs::get_job(self.connection(), job_id)
    }

    pub fn list_jobs(
        &self,
        filter: crate::jobs::JobListFilter<'_>,
    ) -> Result<Vec<crate::jobs::Job>, VaultError> {
        crate::jobs::list_jobs(self.connection(), filter)
    }

    pub fn list_child_jobs(&self, parent_id: Uuid) -> Result<Vec<crate::jobs::Job>, VaultError> {
        crate::jobs::list_child_jobs(self.connection(), parent_id)
    }

    pub fn retry_job(&self, job_id: Uuid) -> Result<crate::jobs::Job, VaultError> {
        crate::jobs::retry_job(self.connection(), job_id)
    }

    pub fn create_archive_job(
        &self,
        target_vault: &str,
        input: YtdlpJobInput,
    ) -> Result<crate::jobs::Job, VaultError> {
        let payload = input.to_json()?;
        crate::jobs::create_job(self.connection(), "yt-dlp", target_vault, &payload)
    }

    pub fn sources_has(
        &self,
        source: &str,
        kind: &str,
        external_ids: &[String],
    ) -> Result<Vec<crate::sources::SourceHasHit>, VaultError> {
        crate::sources::sources_has(self.connection(), source, kind, external_ids)
    }

    pub fn record_source_failure(
        &self,
        input: &crate::failures::RecordFailureInput,
    ) -> Result<crate::failures::SourceFailure, VaultError> {
        crate::failures::record_failure(self.connection(), input)
    }

    pub fn list_source_failures(
        &self,
        source: Option<&str>,
        kind: Option<&str>,
    ) -> Result<Vec<crate::failures::SourceFailure>, VaultError> {
        crate::failures::list_failures(self.connection(), source, kind)
    }

    pub fn list_failures_for_job(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<crate::failures::SourceFailure>, VaultError> {
        crate::failures::list_failures_for_job(self.connection(), job_id)
    }

    pub fn delete_source_failure(&self, id: Uuid) -> Result<(), VaultError> {
        crate::failures::delete_failure(self.connection(), id)
    }

    pub fn create_subscription(
        &self,
        input: &crate::subscriptions::CreateSubscriptionInput,
    ) -> Result<crate::subscriptions::SourceSubscription, VaultError> {
        crate::subscriptions::create_subscription(self.connection(), input)
    }

    pub fn list_subscriptions(&self) -> Result<Vec<crate::subscriptions::SourceSubscription>, VaultError> {
        crate::subscriptions::list_subscriptions(self.connection())
    }

    pub fn delete_subscription(&self, id: Uuid) -> Result<(), VaultError> {
        let sub = crate::subscriptions::get_subscription(self.connection(), id)?
            .ok_or(VaultError::NotFound)?;
        let url = sub.url.clone();
        crate::subscriptions::delete_subscription(self.connection(), id)?;
        crate::jobs::cancel_jobs_for_source_url(self.connection(), &url)?;
        Ok(())
    }

    pub fn run_subscription_now(
        &self,
        id: Uuid,
        target_vault: &str,
    ) -> Result<crate::jobs::Job, VaultError> {
        crate::subscriptions::run_subscription_now(self.connection(), id, target_vault)
    }

    pub fn list_enriched_subscriptions(
        &self,
    ) -> Result<Vec<crate::subscriptions::EnrichedSubscription>, VaultError> {
        let subs = crate::subscriptions::list_subscriptions(self.connection())?;
        crate::subscriptions::enrich_subscriptions(self.connection(), subs)
    }

    pub fn get_media_preferences(&self) -> Result<archiveos_contract::UserMediaPreferences, VaultError> {
        crate::user_preferences::get_media_preferences(self.connection())
    }

    pub fn set_media_preferences(
        &self,
        prefs: &archiveos_contract::UserMediaPreferences,
    ) -> Result<archiveos_contract::UserMediaPreferences, VaultError> {
        crate::user_preferences::set_media_preferences(self.connection(), prefs)
    }

    pub fn refresh_entity_from_source(
        &self,
        target_vault: &str,
        entity_id: Uuid,
        metadata_only: bool,
    ) -> Result<crate::jobs::Job, VaultError> {
        let conn = self.connection();
        let url = crate::import::db::primary_source_url(conn, entity_id)?
            .ok_or(VaultError::NotFound)?;
        let prefs = crate::user_preferences::get_media_preferences(conn)?;
        let asset_policy = if metadata_only {
            let mut policy = archiveos_contract::UserMediaPreferences::metadata_only_policy();
            policy.subtitle_languages = prefs.subtitle_languages;
            policy.subtitles = prefs.subtitle_mode;
            policy.automatic_subtitles = prefs.automatic_subtitles;
            policy
        } else {
            prefs.to_asset_policy()
        };
        self.create_archive_job(
            target_vault,
            YtdlpJobInput {
                url,
                mode: "once".into(),
                resync: true,
                removed_items: "mark_removed".into(),
                asset_policy,
            },
        )
    }

    pub fn refresh_collection_from_source(
        &self,
        target_vault: &str,
        collection_id: Uuid,
    ) -> Result<crate::jobs::Job, VaultError> {
        let conn = self.connection();
        let url = crate::import::db::primary_source_url(conn, collection_id)?
            .ok_or(VaultError::NotFound)?;
        let prefs = crate::user_preferences::get_media_preferences(conn)?;
        self.create_archive_job(
            target_vault,
            YtdlpJobInput {
                url,
                mode: "once".into(),
                resync: true,
                removed_items: "mark_removed".into(),
                asset_policy: prefs.to_asset_policy(),
            },
        )
    }

    pub fn delete_entity(&self, entity_id: Uuid) -> Result<(), VaultError> {
        let urls = crate::import::db::source_ref_urls_for_entity(self.connection(), entity_id)?;
        let assets = crate::assets::list_assets(self.connection(), entity_id)?;
        let changed = self
            .connection()
            .execute(
                "UPDATE entity SET status = 'user_deleted' WHERE id = ?1",
                [entity_id.to_string()],
            )
            .map_err(db_err)?;
        if changed == 0 {
            return Err(VaultError::NotFound);
        }
        crate::assets::mark_entity_assets_deleted(self.connection(), entity_id)?;
        for asset in &assets {
            release_asset_storage(self.connection(), &self.root, asset)?;
        }
        for url in urls {
            crate::jobs::cancel_jobs_for_source_url(self.connection(), &url)?;
        }
        Ok(())
    }

    pub fn delete_asset(&self, asset_id: Uuid) -> Result<(), VaultError> {
        let asset = crate::assets::get_asset(self.connection(), asset_id)?;
        let now = Utc::now().to_rfc3339();
        crate::assets::mark_asset_status(
            self.connection(),
            asset_id,
            "deleted_by_user",
            Some(&now),
        )?;
        release_asset_storage(self.connection(), &self.root, &asset)
    }

    pub fn remove_collection_member(
        &self,
        collection_id: Uuid,
        entity_id: Uuid,
    ) -> Result<(), VaultError> {
        crate::import::db::mark_collection_member_user_removed(
            self.connection(),
            collection_id,
            entity_id,
        )
    }

    pub fn reconcile_assets(&self) -> Result<u32, VaultError> {
        let mut stmt = self
            .connection()
            .prepare(
                "SELECT id, entity_id, role, kind, content_hash, mime, size, ext, status,
                        storage_strategy, path, created_at, updated_at, deleted_at
                 FROM entity_asset
                 WHERE status = 'present'",
            )
            .map_err(db_err)?;
        let assets = stmt
            .query_map([], |row| {
                Ok(crate::assets::EntityAsset {
                    id: parse_uuid(row, 0)?,
                    entity_id: parse_uuid(row, 1)?,
                    role: row.get(2)?,
                    kind: row.get(3)?,
                    content_hash: row.get(4)?,
                    mime: row.get(5)?,
                    size: row.get(6)?,
                    ext: row.get(7)?,
                    status: row.get(8)?,
                    storage_strategy: row.get(9)?,
                    path: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                    deleted_at: row.get(13)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;

        let mut changed = 0;
        for asset in assets {
            if asset.storage_strategy == "remote" {
                continue;
            }
            if asset_storage_exists(&self.root, &asset) {
                continue;
            }
            crate::assets::mark_asset_status(self.connection(), asset.id, "missing_local", None)?;
            changed += 1;
        }
        Ok(changed)
    }

    pub fn reconcile_blobs(
        &self,
        dry_run: bool,
        min_age_secs: u64,
    ) -> Result<archiveos_contract::BlobGcReport, VaultError> {
        crate::cas::sweep_unreferenced_blobs(
            self.connection(),
            &self.root,
            crate::cas::BlobGcOptions {
                dry_run,
                min_age_secs,
            },
        )
    }

    pub fn create_asset_acquisition_job(
        &self,
        target_vault: &str,
        entity_id: Uuid,
        asset_id: Uuid,
    ) -> Result<crate::jobs::Job, VaultError> {
        let asset = crate::assets::get_asset(self.connection(), asset_id)?;
        if !crate::assets::asset_belongs_to_entity(&asset, entity_id) {
            return Err(VaultError::NotFound);
        }
        if asset.status != "remote" && asset.status != "missing_local" {
            return Err(VaultError::InvalidLayout {
                detail: format!(
                    "asset {} has status '{}'; only remote or missing_local can be acquired",
                    asset_id, asset.status
                ),
            });
        }
        let input = serde_json::json!({
            "entity_id": entity_id,
            "asset_id": asset_id,
        });
        crate::jobs::create_job(
            self.connection(),
            "yt-dlp-asset",
            target_vault,
            &input.to_string(),
        )
    }

    pub fn commit_asset_file(
        &self,
        entity_id: Uuid,
        asset_id: Uuid,
        job_id: Uuid,
        relative_path: &str,
    ) -> Result<crate::assets::EntityAsset, VaultError> {
        let asset = crate::assets::get_asset(self.connection(), asset_id)?;
        if !crate::assets::asset_belongs_to_entity(&asset, entity_id) {
            return Err(VaultError::NotFound);
        }

        let staging_file = resolve_staging_file(self.staging_job_dir(&job_id.to_string()), relative_path)?;
        let ext = extension_from_path(&staging_file);
        let ext_for_store = ext.strip_prefix('.').unwrap_or(&ext);
        let cas = self.store_managed(&staging_file, ext_for_store)?;
        let mime = guess_mime(&cas.blob_path);
        let path_str = cas.blob_path.to_string_lossy().into_owned();

        crate::assets::mark_catalog_asset_present(
            self.connection(),
            asset_id,
            &crate::assets::UpsertAssetInput {
                entity_id,
                role: &asset.role,
                kind: &asset.kind,
                content_hash: Some(&cas.content_hash),
                mime: Some(&mime),
                size: cas.size,
                ext: Some(&ext),
                status: "present",
                storage_strategy: "managed",
                path: Some(&path_str),
            },
        )?;

        crate::assets::get_asset(self.connection(), asset_id)
    }

    pub fn process_due_subscriptions(&self) -> Result<Vec<crate::jobs::Job>, VaultError> {
        let conn = self.connection();
        let due = crate::subscriptions::due_subscriptions(conn)?;
        let mut created = Vec::new();
        for sub in due {
            let input = crate::subscriptions::subscription_job_input(&sub)?;
            let job = crate::jobs::create_job(conn, "yt-dlp", &sub.target_vault, &input)?;
            crate::subscriptions::mark_subscription_checked(conn, sub.id, sub.interval_minutes)?;
            created.push(job);
        }
        Ok(created)
    }
}

fn release_asset_storage(
    conn: &Connection,
    vault_root: &Path,
    asset: &crate::assets::EntityAsset,
) -> Result<(), VaultError> {
    match asset.storage_strategy.as_str() {
        "managed" => {
            let Some(hash) = asset.content_hash.as_deref() else {
                return Ok(());
            };
            let _ = crate::cas::remove_blob_if_unreferenced(
                conn,
                vault_root,
                hash,
                asset.ext.as_deref(),
                asset.mime.as_deref(),
            )?;
            Ok(())
        }
        "reference" => remove_reference_asset_file(asset),
        _ => Ok(()),
    }
}

fn remove_reference_asset_file(asset: &crate::assets::EntityAsset) -> Result<(), VaultError> {
    let Some(path) = &asset.path else {
        return Ok(());
    };
    let path = Path::new(path);
    if path.is_file() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn asset_storage_exists(vault_root: &Path, asset: &crate::assets::EntityAsset) -> bool {
    match asset.storage_strategy.as_str() {
        "managed" => asset
            .content_hash
            .as_deref()
            .and_then(|hash| {
                find_blob_path(
                    vault_root,
                    hash,
                    asset.ext.as_deref(),
                    asset.mime.as_deref(),
                )
                .ok()
            })
            .is_some_and(|path| path.is_file()),
        "reference" => asset
            .path
            .as_deref()
            .map(|path| Path::new(path).is_file())
            .unwrap_or(false),
        _ => true,
    }
}

fn resolve_staging_file(staging_dir: PathBuf, relative_path: &str) -> Result<PathBuf, VaultError> {
    if relative_path.is_empty()
        || relative_path.contains('\\')
        || relative_path.starts_with('/')
        || relative_path.contains("..")
    {
        return Err(VaultError::InvalidLayout {
            detail: format!("invalid staging relative path: {relative_path}"),
        });
    }
    let staging_file = staging_dir.join(relative_path);
    let canonical_staging = staging_dir
        .canonicalize()
        .map_err(VaultError::Io)?;
    let canonical_file = staging_file.canonicalize().map_err(|err| {
        VaultError::InvalidLayout {
            detail: format!(
                "staging file not found under {}: {err}",
                staging_dir.display()
            ),
        }
    })?;
    if !canonical_file.starts_with(&canonical_staging) {
        return Err(VaultError::InvalidLayout {
            detail: format!("staging path escapes job directory: {relative_path}"),
        });
    }
    Ok(canonical_file)
}

fn guess_mime(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
}

fn extension_from_path(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default()
}

fn parse_uuid(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&row.get::<_, String>(idx)?).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            Box::new(err),
        )
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
    use archiveos_contract::{VaultError, DB_SCHEMA_VERSION};
    use tempfile::tempdir;

    #[test]
    fn init_creates_expected_layout() {
        let parent = tempdir().unwrap();
        let vault_path = parent.path().join("my-vault");
        let vault = Vault::init(&vault_path).unwrap();

        assert!(vault_path.join(".archiveos/vault.json").is_file());
        assert!(vault_path.join(".archiveos/db.sqlite").is_file());
        assert!(vault_path.join("blobs").is_dir());
        assert!(vault_path.join("staging").is_dir());
        assert!(vault_path.join("inbox").is_dir());
        assert_eq!(vault.root(), vault_path.as_path());
    }

    #[test]
    fn init_applies_schema() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let version: i32 = vault
            .connection()
            .query_row(
                "SELECT MAX(version) FROM schema_migrations",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, DB_SCHEMA_VERSION);
    }

    #[test]
    fn init_fails_on_nonempty_dir() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("existing.txt"), b"x").unwrap();
        assert!(matches!(
            Vault::init(dir.path()),
            Err(VaultError::AlreadyExists)
        ));
    }

    #[test]
    fn open_succeeds_after_init() {
        let parent = tempdir().unwrap();
        let vault_path = parent.path().join("v");
        let v1 = Vault::init(&vault_path).unwrap();
        let v2 = Vault::open(&vault_path).unwrap();
        assert_eq!(v1.id(), v2.id());
    }

    #[test]
    fn open_runs_migrations_idempotently() {
        let dir = tempdir().unwrap();
        Vault::init(dir.path()).unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let count: i32 = vault
            .connection()
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, DB_SCHEMA_VERSION as i32);
    }

    #[test]
    fn open_fails_on_missing_blobs() {
        let dir = tempdir().unwrap();
        Vault::init(dir.path()).unwrap();
        std::fs::remove_dir_all(dir.path().join("blobs")).unwrap();
        assert!(Vault::open(dir.path()).is_err());
    }

    #[test]
    fn store_managed_via_vault() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let staging = vault.staging_job_dir("job-1").join("files/clip.jpg");
        std::fs::create_dir_all(staging.parent().unwrap()).unwrap();
        std::fs::write(&staging, b"image-data").unwrap();

        let result = vault.store_managed(&staging, ".jpg").unwrap();
        assert!(!result.deduped);
        assert!(result.blob_path.starts_with(vault.blobs_dir()));
        assert!(vault.blob_exists(&result.content_hash, ".jpg").unwrap());
    }

    #[test]
    fn delete_entity_marks_tombstone_and_assets_deleted() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        std::fs::write(vault.inbox_dir().join("video.mp4"), b"video").unwrap();
        vault.process_inbox(None).unwrap();

        let entity_id: Uuid = vault
            .connection()
            .query_row("SELECT id FROM entity LIMIT 1", [], |row| {
                let id: String = row.get(0)?;
                Ok(Uuid::parse_str(&id).unwrap())
            })
            .unwrap();

        vault.delete_entity(entity_id).unwrap();

        let status: String = vault
            .connection()
            .query_row(
                "SELECT status FROM entity WHERE id = ?1",
                [entity_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "user_deleted");

        let asset_status: String = vault
            .connection()
            .query_row(
                "SELECT status FROM entity_asset WHERE entity_id = ?1",
                [entity_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(asset_status, "deleted_by_user");
    }

    #[test]
    fn commit_asset_file_promotes_remote_track_to_present() {
        use archiveos_contract::ManifestAssetCatalogEntry;
        use crate::assets::{get_asset, upsert_catalog_asset};
        use crate::import::db::insert_entity;
        use std::collections::BTreeMap;

        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let conn = vault.connection();
        let entity_id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();
        insert_entity(conn, entity_id, None, None, 0, "active", &now, None).unwrap();

        let asset_id = upsert_catalog_asset(
            conn,
            entity_id,
            &ManifestAssetCatalogEntry {
                track_key: "subtitle:en:vtt:manual".into(),
                role: "supporting".into(),
                kind: "subtitle".into(),
                status: "remote".into(),
                storage_strategy: "remote".into(),
                external_id: "vid:subtitle:en:vtt:manual".into(),
                metadata: BTreeMap::from([
                    ("language".into(), serde_json::json!("en")),
                    ("source_url".into(), serde_json::json!("https://example.com/en.vtt")),
                ]),
            },
        )
        .unwrap();

        let job = vault
            .create_asset_acquisition_job("test-vault", entity_id, asset_id)
            .unwrap();
        assert_eq!(job.job_type, "yt-dlp-asset");

        let staging_file = vault
            .staging_job_dir(&job.id.to_string())
            .join("files/en.vtt");
        std::fs::create_dir_all(staging_file.parent().unwrap()).unwrap();
        std::fs::write(&staging_file, b"WEBVTT\n").unwrap();

        let committed = vault
            .commit_asset_file(entity_id, asset_id, job.id, "files/en.vtt")
            .unwrap();
        assert_eq!(committed.status, "present");
        assert!(committed.content_hash.is_some());
        assert_eq!(committed.mime.as_deref(), Some("text/vtt"));

        let reloaded = get_asset(conn, asset_id).unwrap();
        assert_eq!(reloaded.status, "present");
    }

    #[test]
    fn delete_asset_keeps_entity_active() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        std::fs::write(vault.inbox_dir().join("video.mp4"), b"video").unwrap();
        vault.process_inbox(None).unwrap();

        let (entity_id, asset_id): (Uuid, Uuid) = vault
            .connection()
            .query_row(
                "SELECT e.id, a.id FROM entity e JOIN entity_asset a ON a.entity_id = e.id LIMIT 1",
                [],
                |row| {
                    let entity: String = row.get(0)?;
                    let asset: String = row.get(1)?;
                    Ok((Uuid::parse_str(&entity).unwrap(), Uuid::parse_str(&asset).unwrap()))
                },
            )
            .unwrap();

        vault.delete_asset(asset_id).unwrap();

        let status: String = vault
            .connection()
            .query_row(
                "SELECT status FROM entity WHERE id = ?1",
                [entity_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "active");
    }

    #[test]
    fn reconcile_missing_assets_marks_missing_local() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        std::fs::write(vault.inbox_dir().join("video.mp4"), b"video").unwrap();
        vault.process_inbox(None).unwrap();

        let asset_path: String = vault
            .connection()
            .query_row("SELECT path FROM entity_asset LIMIT 1", [], |row| row.get(0))
            .unwrap();
        std::fs::remove_file(asset_path).unwrap();

        let changed = vault.reconcile_assets().unwrap();
        assert_eq!(changed, 1);

        let status: String = vault
            .connection()
            .query_row("SELECT status FROM entity_asset LIMIT 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(status, "missing_local");
    }

    #[test]
    fn reconcile_managed_asset_uses_content_hash() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let hash = "9".repeat(64);
        let blob = crate::layout::blob_path(vault.root(), &hash, ".jpg").unwrap();
        std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
        std::fs::write(&blob, b"thumb").unwrap();

        let entity_id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        crate::import::db::insert_entity(
            vault.connection(),
            entity_id,
            None,
            Some("image/jpeg"),
            5,
            "active",
            &now,
            None,
        )
        .unwrap();
        crate::assets::upsert_asset(
            vault.connection(),
            &crate::assets::UpsertAssetInput {
                entity_id,
                role: "supporting",
                kind: "thumbnail",
                content_hash: Some(&hash),
                mime: Some("image/jpeg"),
                size: 5,
                ext: Some(".jpg"),
                status: "present",
                storage_strategy: "managed",
                path: Some(&blob.to_string_lossy()),
            },
        )
        .unwrap();

        std::fs::remove_file(&blob).unwrap();
        let changed = vault.reconcile_assets().unwrap();
        assert_eq!(changed, 1);
    }

    #[test]
    fn delete_asset_keeps_shared_managed_blob_for_other_entity() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let hash = "8".repeat(64);
        let blob = crate::layout::blob_path(vault.root(), &hash, ".jpg").unwrap();
        std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
        std::fs::write(&blob, b"shared").unwrap();
        let now = Utc::now().to_rfc3339();
        let entity_a = Uuid::new_v4();
        let entity_b = Uuid::new_v4();
        crate::import::db::insert_entity(
            vault.connection(),
            entity_a,
            Some(&hash),
            Some("image/jpeg"),
            6,
            "active",
            &now,
            None,
        )
        .unwrap();
        crate::import::db::insert_entity(
            vault.connection(),
            entity_b,
            None,
            Some("image/jpeg"),
            6,
            "active",
            &now,
            None,
        )
        .unwrap();
        let asset_b = crate::assets::upsert_asset(
            vault.connection(),
            &crate::assets::UpsertAssetInput {
                entity_id: entity_b,
                role: "supporting",
                kind: "thumbnail",
                content_hash: Some(&hash),
                mime: Some("image/jpeg"),
                size: 6,
                ext: Some(".jpg"),
                status: "present",
                storage_strategy: "managed",
                path: Some(&blob.to_string_lossy()),
            },
        )
        .unwrap();

        vault.delete_asset(asset_b).unwrap();
        assert!(blob.exists());
    }
}
