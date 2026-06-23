use std::sync::Arc;

use archiveos_contract::{
    AddUserListMemberInput, AssetPolicy, BrowseQuery, CreateUserListInput, ImportManifest,
    ImportStrategy, LibraryListQuery, PlaybackStateInput, ReorderUserListMembersInput, SearchQuery,
    SubscriptionOptions, UserMediaPreferences, VaultRegistryEntry, YtdlpJobInput,
};
use archiveos_core::{open_vault_ref, Vault};
use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/vaults", get(list_vaults).post(register_vault))
        .route("/vaults/{name}", delete(unregister_vault))
        .route("/vaults/{vault}/entities", get(list_entities))
        .route("/vaults/{vault}/channels/{channel_id}", get(get_channel))
        .route(
            "/vaults/{vault}/channels/{channel_id}/videos",
            get(list_channel_videos),
        )
        .route("/vaults/{vault}/entities/{id}", get(get_entity).delete(delete_entity))
        .route(
            "/vaults/{vault}/entities/{id}/assets/{asset_id}",
            delete(delete_asset),
        )
        .route(
            "/vaults/{vault}/entities/{id}/assets/{asset_id}/acquire",
            post(acquire_asset),
        )
        .route(
            "/vaults/{vault}/entities/{id}/assets/{asset_id}/commit",
            post(commit_asset),
        )
        .route(
            "/vaults/{vault}/assets/{asset_id}/content",
            get(crate::media::asset_content),
        )
        .route("/vaults/{vault}/reconcile/assets", post(reconcile_assets))
        .route("/vaults/{vault}/reconcile/blobs", post(reconcile_blobs))
        .route(
            "/vaults/{vault}/collections/{collection_id}/members/{entity_id}",
            delete(remove_collection_member),
        )
        .route("/vaults/{vault}/search", get(search_entities))
        .route("/vaults/{vault}/entities/{id}/tags", post(add_tag))
        .route(
            "/vaults/{vault}/entities/{id}/tags/{tag}",
            delete(remove_tag),
        )
        .route("/vaults/{vault}/jobs", get(list_jobs).post(create_job))
        .route("/vaults/{vault}/jobs/{id}", get(get_job))
        .route("/vaults/{vault}/jobs/{id}/retry", post(retry_job))
        .route("/vaults/{vault}/jobs/{id}/cancel", post(cancel_job))
        .route("/vaults/{vault}/archive", post(create_archive))
        .route("/vaults/{vault}/subscribe", post(subscribe_archive))
        .route("/vaults/{vault}/jobs/claim", post(claim_job))
        .route("/vaults/{vault}/jobs/{id}/heartbeat", post(heartbeat_job))
        .route("/vaults/{vault}/jobs/{id}/manifest", post(submit_manifest))
        .route("/vaults/{vault}/jobs/{id}/finish", post(finish_job))
        .route("/vaults/{vault}/sources/has", get(sources_has))
        .route("/vaults/{vault}/source-failures", get(list_source_failures).post(record_source_failure))
        .route("/vaults/{vault}/subscriptions", get(list_subscriptions).post(create_subscription))
        .route("/vaults/{vault}/subscriptions/{id}", delete(delete_subscription))
        .route("/vaults/{vault}/subscriptions/{id}/run", post(run_subscription))
        .route(
            "/vaults/{vault}/users/me/media-preferences",
            get(get_media_preferences).put(set_media_preferences),
        )
        .route(
            "/vaults/{vault}/entities/{id}/refresh-from-source",
            post(refresh_entity_from_source),
        )
        .route(
            "/vaults/{vault}/collections/{collection_id}/refresh-from-source",
            post(refresh_collection_from_source),
        )
        .route("/vaults/{vault}/capabilities", get(list_capabilities))
        .route("/vaults/{vault}/collections", get(list_collections))
        .route("/vaults/{vault}/collections/{collection_id}", get(get_collection))
        .route(
            "/vaults/{vault}/collections/{collection_id}/members",
            get(list_collection_members),
        )
        .route("/vaults/{vault}/library/lists", get(list_library_lists))
        .route("/vaults/{vault}/library/lists/{list_id}", get(get_library_list))
        .route(
            "/vaults/{vault}/playback-state/{entity_id}",
            get(get_playback_state).put(upsert_playback_state),
        )
        .route(
            "/vaults/{vault}/playback-state/{entity_id}/dismiss",
            post(dismiss_playback_state),
        )
        .route("/vaults/{vault}/user-lists", post(create_user_list))
        .route(
            "/vaults/{vault}/user-lists/{list_id}/members",
            post(add_user_list_member),
        )
        .route(
            "/vaults/{vault}/user-lists/{list_id}/members/reorder",
            patch(reorder_user_list_members),
        )
        .route(
            "/vaults/{vault}/user-lists/{list_id}/members/{entity_id}",
            delete(remove_user_list_member),
        )
        .route(
            "/vaults/{vault}/entities/{id}/previews/regenerate",
            post(regenerate_previews),
        )
        .route("/vaults/{vault}/previews/backfill", post(backfill_previews))
        .route(
            "/vaults/{vault}/jobs/{id}/preview-commit",
            post(commit_preview_job),
        )
}

async fn health() -> &'static str {
    "ok"
}

async fn list_vaults(State(state): State<Arc<AppState>>) -> ApiResult<Json<Vec<VaultRegistryEntry>>> {
    let registry = state.registry()?;
    Ok(Json(registry.refresh()?))
}

#[derive(Deserialize)]
struct RegisterVaultRequest {
    name: String,
    path: String,
    /// Create vault layout at `path` when missing (Docker / first-run).
    #[serde(default)]
    init: bool,
}

async fn register_vault(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterVaultRequest>,
) -> ApiResult<Json<VaultRegistryEntry>> {
    ensure_vault_ready(&body.path, body.init)?;
    let registry = state.registry()?;
    Ok(Json(registry.register(&body.name, &body.path)?))
}

fn ensure_vault_ready(path: &str, init: bool) -> ApiResult<()> {
    if !init {
        return Ok(());
    }
    let path = std::path::Path::new(path);
    if Vault::open(path).is_err() {
        Vault::ensure(path)?;
    }
    Ok(())
}

async fn unregister_vault(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    state.registry()?.unregister(&name)?;
    Ok(Json(serde_json::json!({ "removed": name })))
}

pub(crate) fn open_vault(state: &AppState, vault: &str) -> Result<Vault, ApiError> {
    let registry = state.registry()?;
    Ok(open_vault_ref(Some(&registry), vault)?)
}

async fn get_entity(
    State(state): State<Arc<AppState>>,
    Path((vault, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<EntityDetailResponse>> {
    let vault = open_vault(&state, &vault)?;
    let detail = vault.get_entity(id)?;
    Ok(Json(EntityDetailResponse::from(detail)))
}

async fn delete_entity(
    State(state): State<Arc<AppState>>,
    Path((vault, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault)?;
    vault.delete_entity(id)?;
    Ok(Json(serde_json::json!({ "entity_id": id, "status": "user_deleted" })))
}

async fn delete_asset(
    State(state): State<Arc<AppState>>,
    Path((vault, _id, asset_id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault)?;
    vault.delete_asset(asset_id)?;
    Ok(Json(serde_json::json!({ "asset_id": asset_id, "status": "deleted_by_user" })))
}

async fn acquire_asset(
    State(state): State<Arc<AppState>>,
    Path((vault_name, entity_id, asset_id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<JobResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.create_asset_acquisition_job(&vault_name, entity_id, asset_id)?;
    Ok(Json(JobResponse::from(job)))
}

#[derive(Deserialize)]
struct CommitAssetRequest {
    job_id: Uuid,
    path: String,
}

async fn commit_asset(
    State(state): State<Arc<AppState>>,
    Path((vault_name, entity_id, asset_id)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<CommitAssetRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    let asset = vault.commit_asset_file(entity_id, asset_id, body.job_id, &body.path)?;
    vault.finish_job(body.job_id, "done")?;
    Ok(Json(serde_json::json!({
        "entity_id": entity_id,
        "asset_id": asset.id,
        "status": asset.status,
        "content_hash": asset.content_hash,
        "path": asset.path,
    })))
}

async fn reconcile_assets(
    State(state): State<Arc<AppState>>,
    Path(vault): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault)?;
    let changed = vault.reconcile_assets()?;
    Ok(Json(serde_json::json!({ "missing_local": changed })))
}

#[derive(Deserialize)]
struct ReconcileBlobsParams {
    #[serde(default = "default_blob_gc_dry_run")]
    dry_run: bool,
    min_age_secs: Option<u64>,
}

fn default_blob_gc_dry_run() -> bool {
    true
}

async fn reconcile_blobs(
    State(state): State<Arc<AppState>>,
    Path(vault): Path<String>,
    Query(params): Query<ReconcileBlobsParams>,
) -> ApiResult<Json<archiveos_contract::BlobGcReport>> {
    let vault = open_vault(&state, &vault)?;
    let min_age_secs = params.min_age_secs.unwrap_or_else(default_blob_gc_min_age_secs);
    let report = vault.reconcile_blobs(params.dry_run, min_age_secs)?;
    Ok(Json(report))
}

fn default_blob_gc_min_age_secs() -> u64 {
    std::env::var("ARCHIVEOS_BLOB_GC_MIN_AGE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(86_400)
}

async fn remove_collection_member(
    State(state): State<Arc<AppState>>,
    Path((vault, collection_id, entity_id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault)?;
    vault.remove_collection_member(collection_id, entity_id)?;
    Ok(Json(serde_json::json!({
        "collection_id": collection_id,
        "entity_id": entity_id,
        "status": "user_removed"
    })))
}

#[derive(Serialize)]
struct EntityDetailResponse {
    id: Uuid,
    /// Legacy read-model fields. Canonical bytes and renditions live in `assets`.
    content_hash: Option<String>,
    /// Legacy read-model fields. Canonical bytes and renditions live in `assets`.
    mime: Option<String>,
    /// Legacy read-model fields. Canonical bytes and renditions live in `assets`.
    size: i64,
    status: String,
    added_at: String,
    created_at: Option<String>,
    tags: Vec<String>,
    metadata: std::collections::HashMap<String, String>,
    metadata_entries: Vec<archiveos_contract::MetadataEntry>,
    assets: Vec<EntityAssetResponse>,
    preview: Option<EntityPreviewResponse>,
    timeline_sprite: Option<EntityPreviewResponse>,
    timeline_manifest: Option<EntityPreviewResponse>,
}

#[derive(Serialize)]
struct EntityPreviewResponse {
    entity_id: Uuid,
    asset_id: Uuid,
    kind: String,
    preview_role: String,
    status: String,
}

impl From<archiveos_core::entity::EntityPreview> for EntityPreviewResponse {
    fn from(p: archiveos_core::entity::EntityPreview) -> Self {
        Self {
            entity_id: p.entity_id,
            asset_id: p.asset_id,
            kind: p.kind,
            preview_role: p.preview_role,
            status: p.status,
        }
    }
}

impl From<archiveos_core::entity::EntityDetail> for EntityDetailResponse {
    fn from(d: archiveos_core::entity::EntityDetail) -> Self {
        Self {
            id: d.id,
            content_hash: d.content_hash,
            mime: d.mime,
            size: d.size,
            status: d.status,
            added_at: d.added_at,
            created_at: d.created_at,
            tags: d.tags,
            metadata: d.metadata,
            metadata_entries: d.metadata_entries,
            assets: d.assets.into_iter().map(EntityAssetResponse::from).collect(),
            preview: d.preview.map(EntityPreviewResponse::from),
            timeline_sprite: d.timeline_sprite.map(EntityPreviewResponse::from),
            timeline_manifest: d.timeline_manifest.map(EntityPreviewResponse::from),
        }
    }
}

#[derive(Serialize)]
struct EntityAssetResponse {
    id: Uuid,
    role: String,
    kind: String,
    content_hash: Option<String>,
    mime: Option<String>,
    size: i64,
    ext: Option<String>,
    status: String,
    storage_strategy: String,
    path: Option<String>,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
    metadata: std::collections::HashMap<String, String>,
    metadata_entries: Vec<archiveos_contract::MetadataEntry>,
}

impl From<archiveos_core::assets::EntityAssetWithMetadata> for EntityAssetResponse {
    fn from(item: archiveos_core::assets::EntityAssetWithMetadata) -> Self {
        let asset = item.asset;
        Self {
            id: asset.id,
            role: asset.role,
            kind: asset.kind,
            content_hash: asset.content_hash,
            mime: asset.mime,
            size: asset.size,
            ext: asset.ext,
            status: asset.status,
            storage_strategy: asset.storage_strategy,
            path: asset.path,
            created_at: asset.created_at,
            updated_at: asset.updated_at,
            deleted_at: asset.deleted_at,
            metadata: item.metadata,
            metadata_entries: item.metadata_entries,
        }
    }
}

#[derive(Deserialize)]
struct BrowseParams {
    query: Option<String>,
    kind: Option<String>,
    source: Option<String>,
    exclude_source: Option<String>,
    status: Option<String>,
    #[serde(default = "default_browse_limit")]
    limit: u32,
    #[serde(default)]
    include_hidden: bool,
    sort: Option<String>,
    uploaded_by: Option<Uuid>,
}

fn default_browse_limit() -> u32 {
    50
}

async fn list_entities(
    State(state): State<Arc<AppState>>,
    Path(vault): Path<String>,
    Query(params): Query<BrowseParams>,
) -> ApiResult<Json<Vec<archiveos_contract::EntityListItem>>> {
    let vault = open_vault(&state, &vault)?;
    let items = vault.browse(&BrowseQuery {
        text: params.query,
        kind: params.kind,
        source: params.source,
        exclude_source: params.exclude_source,
        status: params.status,
        limit: params.limit,
        include_hidden: params.include_hidden,
        sort: params.sort,
        uploaded_by: params.uploaded_by,
    })?;
    Ok(Json(items))
}

async fn get_channel(
    State(state): State<Arc<AppState>>,
    Path((vault, channel_id)): Path<(String, Uuid)>,
) -> ApiResult<Json<archiveos_contract::ChannelDetail>> {
    let vault = open_vault(&state, &vault)?;
    Ok(Json(vault.get_channel(channel_id)?))
}

#[derive(Deserialize)]
struct ChannelVideosParams {
    #[serde(default = "default_browse_limit")]
    limit: u32,
    sort: Option<String>,
}

async fn list_channel_videos(
    State(state): State<Arc<AppState>>,
    Path((vault, channel_id)): Path<(String, Uuid)>,
    Query(params): Query<ChannelVideosParams>,
) -> ApiResult<Json<Vec<archiveos_contract::EntityListItem>>> {
    let vault = open_vault(&state, &vault)?;
    let items = vault.list_channel_videos(channel_id, params.limit, params.sort.as_deref())?;
    Ok(Json(items))
}

#[derive(Deserialize)]
struct SearchParams {
    tag: Option<String>,
    query: Option<String>,
    #[serde(default)]
    include_hidden: bool,
}

async fn search_entities(
    State(state): State<Arc<AppState>>,
    Path(vault): Path<String>,
    Query(params): Query<SearchParams>,
) -> ApiResult<Json<Vec<archiveos_contract::EntityHit>>> {
    let vault = open_vault(&state, &vault)?;
    let hits = vault.search(&SearchQuery {
        tag: params.tag,
        text: params.query,
        include_hidden: params.include_hidden,
    })?;
    Ok(Json(hits))
}

#[derive(Deserialize)]
struct TagRequest {
    name: String,
}

async fn add_tag(
    State(state): State<Arc<AppState>>,
    Path((vault, id)): Path<(String, Uuid)>,
    Json(body): Json<TagRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault)?;
    vault.add_tag(id, &body.name)?;
    Ok(Json(serde_json::json!({ "entity_id": id, "tag": body.name })))
}

async fn remove_tag(
    State(state): State<Arc<AppState>>,
    Path((vault, id, tag)): Path<(String, Uuid, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault)?;
    vault.remove_tag(id, &tag)?;
    Ok(Json(serde_json::json!({ "entity_id": id, "tag": tag })))
}

#[derive(Deserialize)]
struct CreateJobRequest {
    #[serde(rename = "type")]
    job_type: String,
    input: String,
}

async fn create_job(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Json(body): Json<CreateJobRequest>,
) -> ApiResult<Json<JobResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.create_job(&body.job_type, &vault_name, &body.input)?;
    Ok(Json(JobResponse::from(job)))
}

#[derive(Deserialize)]
struct ArchiveRequest {
    url: String,
    #[serde(default = "default_mode_once")]
    mode: String,
    #[serde(default = "default_true")]
    resync: bool,
    #[serde(default = "default_removed_items")]
    removed_items: String,
    #[serde(default)]
    asset_policy: AssetPolicy,
}

fn default_mode_once() -> String {
    "once".into()
}

fn default_removed_items() -> String {
    "mark_removed".into()
}

fn default_true() -> bool {
    true
}

async fn create_archive(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Json(body): Json<ArchiveRequest>,
) -> ApiResult<Json<JobResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.create_archive_job(
        &vault_name,
        YtdlpJobInput {
            url: body.url,
            mode: body.mode,
            resync: body.resync,
            removed_items: body.removed_items,
            asset_policy: body.asset_policy,
        },
    )?;
    Ok(Json(JobResponse::from(job)))
}

#[derive(Deserialize)]
struct SubscribeArchiveRequest {
    source: String,
    kind: String,
    url: String,
    interval_minutes: i32,
    #[serde(default = "default_true")]
    resync: bool,
    #[serde(default = "default_removed_items")]
    removed_items: String,
    #[serde(default)]
    asset_policy: AssetPolicy,
}

async fn subscribe_archive(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Json(body): Json<SubscribeArchiveRequest>,
) -> ApiResult<Json<SubscriptionResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let sub = vault.create_subscription(&archiveos_core::subscriptions::CreateSubscriptionInput {
        source: body.source,
        kind: body.kind,
        url: body.url,
        target_vault: vault_name,
        interval_minutes: body.interval_minutes,
        options: Some(SubscriptionOptions {
            resync: body.resync,
            removed_items: body.removed_items,
            asset_policy: body.asset_policy,
            ..Default::default()
        }),
    })?;
    Ok(Json(SubscriptionResponse::from(sub)))
}

#[derive(Deserialize)]
struct ListJobsParams {
    status: Option<String>,
    #[serde(rename = "type")]
    job_type: Option<String>,
    #[serde(default)]
    root_only: bool,
    #[serde(default = "default_job_limit")]
    limit: i32,
}

fn default_job_limit() -> i32 {
    50
}

async fn list_jobs(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Query(params): Query<ListJobsParams>,
) -> ApiResult<Json<Vec<JobResponse>>> {
    let vault = open_vault(&state, &vault_name)?;
    let jobs = vault.list_jobs(archiveos_core::jobs::JobListFilter {
        status: params.status.as_deref(),
        job_type: params.job_type.as_deref(),
        root_only: params.root_only,
        limit: params.limit,
    })?;
    Ok(Json(jobs.into_iter().map(JobResponse::from).collect()))
}

#[derive(Deserialize)]
struct GetJobParams {
    #[serde(default)]
    include_children: bool,
}

#[derive(Serialize)]
struct JobDetailResponse {
    #[serde(flatten)]
    job: JobResponse,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<JobResponse>,
}

async fn get_job(
    State(state): State<Arc<AppState>>,
    Path((vault_name, job_id)): Path<(String, Uuid)>,
    Query(params): Query<GetJobParams>,
) -> ApiResult<Json<JobDetailResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.get_job(job_id)?.ok_or(archiveos_contract::VaultError::NotFound)?;
    let children = if params.include_children {
        vault
            .list_child_jobs(job_id)?
            .into_iter()
            .map(JobResponse::from)
            .collect()
    } else {
        Vec::new()
    };
    Ok(Json(JobDetailResponse {
        job: JobResponse::from(job),
        children,
    }))
}

async fn retry_job(
    State(state): State<Arc<AppState>>,
    Path((vault_name, job_id)): Path<(String, Uuid)>,
) -> ApiResult<Json<JobResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.retry_job(job_id)?;
    Ok(Json(JobResponse::from(job)))
}

async fn cancel_job(
    State(state): State<Arc<AppState>>,
    Path((vault_name, job_id)): Path<(String, Uuid)>,
) -> ApiResult<Json<JobResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.cancel_job(job_id)?;
    Ok(Json(JobResponse::from(job)))
}

#[derive(Serialize)]
struct JobResponse {
    id: Uuid,
    #[serde(rename = "type")]
    job_type: String,
    target_vault: String,
    input: String,
    status: String,
    lease_until: Option<String>,
    attempts: i32,
    created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<archiveos_contract::JobProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_job_id: Option<Uuid>,
}

impl From<archiveos_core::jobs::Job> for JobResponse {
    fn from(j: archiveos_core::jobs::Job) -> Self {
        Self {
            id: j.id,
            job_type: j.job_type,
            target_vault: j.target_vault,
            input: j.input,
            status: j.status,
            lease_until: j.lease_until,
            attempts: j.attempts,
            created_at: j.created_at,
            progress: j.progress,
            parent_job_id: j.parent_job_id,
        }
    }
}

#[derive(Deserialize)]
struct ClaimJobRequest {
    #[serde(default = "default_lease")]
    lease_secs: i64,
    #[serde(rename = "type")]
    job_type: Option<String>,
}

fn default_lease() -> i64 {
    300
}

async fn claim_job(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    body: Option<Json<ClaimJobRequest>>,
) -> ApiResult<Json<Option<JobResponse>>> {
    let vault = open_vault(&state, &vault_name)?;
    let (lease_secs, job_type) = body
        .map(|b| (b.lease_secs, b.job_type.clone()))
        .unwrap_or_else(|| (default_lease(), None));
    let job = vault.claim_job(lease_secs, job_type.as_deref())?;
    Ok(Json(job.map(JobResponse::from)))
}

#[derive(Deserialize)]
struct HeartbeatJobRequest {
    #[serde(default = "default_lease")]
    lease_secs: i64,
    progress: Option<archiveos_contract::JobProgress>,
}

async fn heartbeat_job(
    State(state): State<Arc<AppState>>,
    Path((vault_name, job_id)): Path<(String, Uuid)>,
    body: Option<Json<HeartbeatJobRequest>>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    let lease_secs = body
        .as_ref()
        .map(|b| b.lease_secs)
        .unwrap_or_else(default_lease);
    let progress = body.as_ref().and_then(|b| b.progress.clone());
    let cancelled = vault.heartbeat_job(job_id, lease_secs, progress.as_ref())?;
    Ok(Json(serde_json::json!({ "job_id": job_id, "ok": true, "cancelled": cancelled })))
}

#[derive(Deserialize)]
struct SubmitManifestRequest {
    #[serde(flatten)]
    manifest: ImportManifest,
    #[serde(default)]
    status: Option<String>,
}

fn resolve_job_status(manifest: &ImportManifest, explicit: Option<&str>) -> String {
    if let Some(status) = explicit {
        return match status {
            "done" | "partial" | "failed" => status.to_string(),
            _ => "done".to_string(),
        };
    }

    let complete = manifest.items.iter().filter(|i| i.status == "complete").count();
    let failed = manifest
        .items
        .iter()
        .filter(|i| i.status == "failed")
        .count();
    if failed > 0 && complete > 0 {
        "partial".to_string()
    } else if failed > 0 && complete == 0 {
        "failed".to_string()
    } else {
        "done".to_string()
    }
}

async fn submit_manifest(
    State(state): State<Arc<AppState>>,
    Path((vault_name, job_id)): Path<(String, Uuid)>,
    Json(body): Json<SubmitManifestRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.get_job(job_id)?;
    if job.is_none() {
        return Err(archiveos_contract::VaultError::NotFound.into());
    }

    let staging = vault.staging_job_dir(&job_id.to_string());
    let report = vault.import(&staging, Some(&body.manifest), ImportStrategy::Managed)?;
    let final_status = resolve_job_status(&body.manifest, body.status.as_deref());
    vault.finish_job(job_id, &final_status)?;
    enqueue_preview_jobs_from_manifest(&vault, &vault_name, &body.manifest, job_id)?;
    maybe_bind_subscription_collection(&vault, job_id, &body.manifest)?;

    Ok(Json(serde_json::json!({
        "job_id": job_id,
        "status": final_status,
        "entities_created": report.entities_created,
        "entities_reused": report.entities_reused,
        "blobs_stored": report.blobs_stored,
        "items_skipped": report.items_skipped,
    })))
}

#[derive(Deserialize)]
struct FinishJobRequest {
    #[serde(default = "default_finish_status")]
    status: String,
}

fn default_finish_status() -> String {
    "done".into()
}

async fn finish_job(
    State(state): State<Arc<AppState>>,
    Path((vault_name, job_id)): Path<(String, Uuid)>,
    body: Option<Json<FinishJobRequest>>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    if vault.get_job(job_id)?.is_none() {
        return Err(archiveos_contract::VaultError::NotFound.into());
    }
    let status = body
        .map(|b| b.status.clone())
        .unwrap_or_else(default_finish_status);
    let final_status = match status.as_str() {
        "done" | "partial" | "failed" | "cancelled" => status,
        _ => "done".to_string(),
    };
    vault.finish_job(job_id, &final_status)?;
    Ok(Json(serde_json::json!({ "job_id": job_id, "status": final_status })))
}

#[derive(Deserialize)]
struct SourcesHasParams {
    source: String,
    kind: String,
    ids: String,
}

async fn sources_has(
    State(state): State<Arc<AppState>>,
    Path(vault): Path<String>,
    Query(params): Query<SourcesHasParams>,
) -> ApiResult<Json<Vec<SourceHasResponse>>> {
    let vault = open_vault(&state, &vault)?;
    let ids: Vec<String> = params
        .ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let hits = vault.sources_has(&params.source, &params.kind, &ids)?;
    Ok(Json(
        hits.into_iter()
            .map(|h| SourceHasResponse {
                external_id: h.external_id,
                entity_id: h.entity_id,
                present: h.present,
                known: h.known,
                entity_status: h.entity_status,
                source_status: h.source_status,
                has_present_asset: h.has_present_asset,
                asset_statuses: h.asset_statuses,
            })
            .collect(),
    ))
}

#[derive(Serialize)]
struct SourceHasResponse {
    external_id: String,
    entity_id: String,
    present: bool,
    known: bool,
    entity_status: String,
    source_status: String,
    has_present_asset: bool,
    asset_statuses: Vec<String>,
}

#[derive(Deserialize)]
struct RecordSourceFailureRequest {
    #[serde(default)]
    job_id: Option<Uuid>,
    pub source: String,
    pub kind: String,
    pub external_id: String,
    #[serde(default)]
    pub url: Option<String>,
    pub stage: String,
    pub error_kind: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Serialize)]
struct SourceFailureResponse {
    id: Uuid,
    job_id: Option<Uuid>,
    source: String,
    kind: String,
    external_id: String,
    url: Option<String>,
    stage: String,
    error_kind: String,
    message: String,
    retryable: bool,
    attempts: i32,
    last_seen_at: String,
    created_at: String,
}

impl From<archiveos_core::failures::SourceFailure> for SourceFailureResponse {
    fn from(f: archiveos_core::failures::SourceFailure) -> Self {
        Self {
            id: f.id,
            job_id: f.job_id,
            source: f.source,
            kind: f.kind,
            external_id: f.external_id,
            url: f.url,
            stage: f.stage,
            error_kind: f.error_kind,
            message: f.message,
            retryable: f.retryable,
            attempts: f.attempts,
            last_seen_at: f.last_seen_at,
            created_at: f.created_at,
        }
    }
}

#[derive(Deserialize)]
struct ListSourceFailuresParams {
    source: Option<String>,
    kind: Option<String>,
}

async fn record_source_failure(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Json(body): Json<RecordSourceFailureRequest>,
) -> ApiResult<Json<SourceFailureResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let failure = vault.record_source_failure(&archiveos_core::failures::RecordFailureInput {
        job_id: body.job_id,
        source: body.source,
        kind: body.kind,
        external_id: body.external_id,
        url: body.url,
        stage: body.stage,
        error_kind: body.error_kind,
        message: body.message,
        retryable: body.retryable,
    })?;
    Ok(Json(SourceFailureResponse::from(failure)))
}

async fn list_source_failures(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Query(params): Query<ListSourceFailuresParams>,
) -> ApiResult<Json<Vec<SourceFailureResponse>>> {
    let vault = open_vault(&state, &vault_name)?;
    let failures = vault.list_source_failures(params.source.as_deref(), params.kind.as_deref())?;
    Ok(Json(failures.into_iter().map(SourceFailureResponse::from).collect()))
}

#[derive(Deserialize)]
struct CreateSubscriptionRequest {
    pub source: String,
    pub kind: String,
    pub url: String,
    pub interval_minutes: i32,
    #[serde(default = "default_true")]
    pub resync: bool,
    #[serde(default = "default_removed_items")]
    pub removed_items: String,
    #[serde(default)]
    pub asset_policy: AssetPolicy,
}

#[derive(Serialize)]
struct SubscriptionResponse {
    id: Uuid,
    source: String,
    kind: String,
    url: String,
    target_vault: String,
    interval_minutes: i32,
    next_run_at: String,
    last_checked_at: Option<String>,
    status: String,
    created_at: String,
    options: Option<SubscriptionOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collection_title: Option<String>,
}

impl From<archiveos_core::subscriptions::EnrichedSubscription> for SubscriptionResponse {
    fn from(e: archiveos_core::subscriptions::EnrichedSubscription) -> Self {
        let s = e.subscription;
        Self {
            id: s.id,
            source: s.source,
            kind: s.kind,
            url: s.url,
            target_vault: s.target_vault,
            interval_minutes: s.interval_minutes,
            next_run_at: s.next_run_at,
            last_checked_at: s.last_checked_at,
            status: s.status,
            created_at: s.created_at,
            options: s.options,
            collection_id: e.collection_id,
            collection_title: e.collection_title,
        }
    }
}

impl From<archiveos_core::subscriptions::SourceSubscription> for SubscriptionResponse {
    fn from(s: archiveos_core::subscriptions::SourceSubscription) -> Self {
        Self {
            id: s.id,
            source: s.source,
            kind: s.kind,
            url: s.url,
            target_vault: s.target_vault,
            interval_minutes: s.interval_minutes,
            next_run_at: s.next_run_at,
            last_checked_at: s.last_checked_at,
            status: s.status,
            created_at: s.created_at,
            options: s.options,
            collection_id: None,
            collection_title: None,
        }
    }
}

async fn create_subscription(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Json(body): Json<CreateSubscriptionRequest>,
) -> ApiResult<Json<SubscriptionResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let sub = vault.create_subscription(&archiveos_core::subscriptions::CreateSubscriptionInput {
        source: body.source,
        kind: body.kind,
        url: body.url,
        target_vault: vault_name,
        interval_minutes: body.interval_minutes,
        options: Some(SubscriptionOptions {
            resync: body.resync,
            removed_items: body.removed_items,
            asset_policy: body.asset_policy,
            ..Default::default()
        }),
    })?;
    Ok(Json(SubscriptionResponse::from(sub)))
}

async fn list_subscriptions(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
) -> ApiResult<Json<Vec<SubscriptionResponse>>> {
    let vault = open_vault(&state, &vault_name)?;
    let subs = vault.list_enriched_subscriptions()?;
    Ok(Json(subs.into_iter().map(SubscriptionResponse::from).collect()))
}

async fn run_subscription(
    State(state): State<Arc<AppState>>,
    Path((vault_name, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<JobResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.run_subscription_now(id, &vault_name)?;
    Ok(Json(JobResponse::from(job)))
}

async fn delete_subscription(
    State(state): State<Arc<AppState>>,
    Path((vault_name, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    vault.delete_subscription(id)?;
    Ok(Json(serde_json::json!({ "removed": id })))
}

async fn get_media_preferences(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
) -> ApiResult<Json<UserMediaPreferences>> {
    let vault = open_vault(&state, &vault_name)?;
    Ok(Json(vault.get_media_preferences()?))
}

async fn set_media_preferences(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Json(body): Json<UserMediaPreferences>,
) -> ApiResult<Json<UserMediaPreferences>> {
    let vault = open_vault(&state, &vault_name)?;
    Ok(Json(vault.set_media_preferences(&body)?))
}

#[derive(Deserialize)]
struct RefreshFromSourceRequest {
    #[serde(default)]
    metadata_only: bool,
}

async fn refresh_entity_from_source(
    State(state): State<Arc<AppState>>,
    Path((vault_name, entity_id)): Path<(String, Uuid)>,
    body: Option<Json<RefreshFromSourceRequest>>,
) -> ApiResult<Json<JobResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let metadata_only = body.map(|b| b.metadata_only).unwrap_or(false);
    let job = vault.refresh_entity_from_source(&vault_name, entity_id, metadata_only)?;
    Ok(Json(JobResponse::from(job)))
}

async fn refresh_collection_from_source(
    State(state): State<Arc<AppState>>,
    Path((vault_name, collection_id)): Path<(String, Uuid)>,
) -> ApiResult<Json<JobResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.refresh_collection_from_source(&vault_name, collection_id)?;
    Ok(Json(JobResponse::from(job)))
}

#[derive(Serialize)]
struct VaultCapabilitiesResponse {
    workers: Vec<archiveos_core::VaultCapability>,
}

async fn list_capabilities(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
) -> ApiResult<Json<VaultCapabilitiesResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    Ok(Json(VaultCapabilitiesResponse {
        workers: vault.vault_capabilities(),
    }))
}

#[derive(Serialize)]
struct CollectionSummaryResponse {
    id: Uuid,
    collection_type: String,
    title: String,
    member_count: i32,
    cover_preview: Option<EntityPreviewResponse>,
}

impl From<archiveos_core::collections::CollectionSummary> for CollectionSummaryResponse {
    fn from(value: archiveos_core::collections::CollectionSummary) -> Self {
        Self {
            id: value.id,
            collection_type: value.collection_type,
            title: value.title,
            member_count: value.member_count,
            cover_preview: value.cover_preview.map(preview_summary_response),
        }
    }
}

#[derive(Deserialize)]
struct CollectionListParams {
    #[serde(rename = "type")]
    collection_type: Option<String>,
    min_member_count: Option<i32>,
}

async fn list_collections(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Query(params): Query<CollectionListParams>,
) -> ApiResult<Json<Vec<CollectionSummaryResponse>>> {
    let vault = open_vault(&state, &vault_name)?;
    let query = archiveos_core::collections::CollectionQuery {
        collection_type: params.collection_type,
        min_member_count: params.min_member_count,
    };
    Ok(Json(
        vault
            .list_collections(&query)?
            .into_iter()
            .map(CollectionSummaryResponse::from)
            .collect(),
    ))
}

#[derive(Serialize)]
struct CollectionDetailResponse {
    id: Uuid,
    collection_type: String,
    title: String,
    member_count: i32,
    cover_preview: Option<EntityPreviewResponse>,
    members: Vec<CollectionMemberResponse>,
}

impl From<archiveos_core::collections::CollectionDetail> for CollectionDetailResponse {
    fn from(value: archiveos_core::collections::CollectionDetail) -> Self {
        Self {
            id: value.id,
            collection_type: value.collection_type,
            title: value.title,
            member_count: value.member_count,
            cover_preview: value.cover_preview.map(preview_summary_response),
            members: value
                .members
                .into_iter()
                .map(CollectionMemberResponse::from)
                .collect(),
        }
    }
}

async fn get_collection(
    State(state): State<Arc<AppState>>,
    Path((vault_name, collection_id)): Path<(String, Uuid)>,
) -> ApiResult<Json<CollectionDetailResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    Ok(Json(
        vault
            .get_collection(collection_id)?
            .into(),
    ))
}

#[derive(Serialize)]
struct CollectionMemberResponse {
    id: Uuid,
    title: Option<String>,
    kind: Option<String>,
    mime: Option<String>,
    source: Option<String>,
    status: String,
    position: i32,
    preview: Option<EntityPreviewResponse>,
    timeline_sprite: Option<EntityPreviewResponse>,
    timeline_manifest: Option<EntityPreviewResponse>,
    primary_asset_id: Option<Uuid>,
    primary_asset_status: Option<String>,
    duration: Option<String>,
    channel: Option<String>,
    uploader: Option<String>,
    webpage_url: Option<String>,
}

fn preview_summary_response(preview: archiveos_contract::EntityPreviewSummary) -> EntityPreviewResponse {
    EntityPreviewResponse {
        entity_id: preview.entity_id,
        asset_id: preview.asset_id,
        kind: preview.kind,
        preview_role: preview.preview_role,
        status: preview.status,
    }
}

impl From<archiveos_core::collections::CollectionMemberItem> for CollectionMemberResponse {
    fn from(value: archiveos_core::collections::CollectionMemberItem) -> Self {
        Self {
            id: value.id,
            title: value.title,
            kind: value.kind,
            mime: value.mime,
            source: value.source,
            status: value.status,
            position: value.position,
            preview: value.preview.map(preview_summary_response),
            timeline_sprite: value.timeline_sprite.map(preview_summary_response),
            timeline_manifest: value.timeline_manifest.map(preview_summary_response),
            primary_asset_id: value.primary_asset_id,
            primary_asset_status: value.primary_asset_status,
            duration: value.duration,
            channel: value.channel,
            uploader: value.uploader,
            webpage_url: value.webpage_url,
        }
    }
}

async fn list_collection_members(
    State(state): State<Arc<AppState>>,
    Path((vault_name, collection_id)): Path<(String, Uuid)>,
) -> ApiResult<Json<Vec<CollectionMemberResponse>>> {
    let vault = open_vault(&state, &vault_name)?;
    Ok(Json(
        vault
            .list_collection_members(collection_id)?
            .into_iter()
            .map(CollectionMemberResponse::from)
            .collect(),
    ))
}

#[derive(Deserialize)]
struct LibraryListParams {
    section: Option<String>,
    source: Option<String>,
    kind: Option<String>,
}

#[derive(Deserialize)]
struct LibraryListDetailParams {
    sort: Option<String>,
    section: Option<String>,
    source: Option<String>,
    kind: Option<String>,
}

async fn list_library_lists(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Query(params): Query<LibraryListParams>,
) -> ApiResult<Json<Vec<archiveos_contract::LibraryListSummary>>> {
    let vault = open_vault(&state, &vault_name)?;
    Ok(Json(vault.list_library_lists(&library_query_from_params(params))?))
}

fn library_query_from_params(params: LibraryListParams) -> LibraryListQuery {
    let mut source = params.source;
    let mut kind = params.kind;
    match params.section.as_deref() {
        Some("youtube") => {
            source = Some("youtube".into());
            kind = Some("video".into());
        }
        Some("videos") => kind = Some("video".into()),
        Some("music") => kind = Some("audio".into()),
        Some("pictures") => kind = Some("image".into()),
        _ => {}
    }
    LibraryListQuery {
        section: params.section,
        source,
        kind,
    }
}

async fn get_library_list(
    State(state): State<Arc<AppState>>,
    Path((vault_name, list_id)): Path<(String, String)>,
    Query(params): Query<LibraryListDetailParams>,
) -> ApiResult<Json<archiveos_contract::LibraryListDetail>> {
    let vault = open_vault(&state, &vault_name)?;
    let scope = library_query_from_params(LibraryListParams {
        section: params.section,
        source: params.source,
        kind: params.kind,
    });
    Ok(Json(
        vault.get_library_list(&list_id, params.sort.as_deref(), Some(&scope))?,
    ))
}

async fn get_playback_state(
    State(state): State<Arc<AppState>>,
    Path((vault_name, entity_id)): Path<(String, Uuid)>,
) -> ApiResult<Json<archiveos_contract::PlaybackStateResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    vault
        .get_playback_state(entity_id)?
        .map(Json)
        .ok_or_else(|| archiveos_contract::VaultError::NotFound.into())
}

async fn upsert_playback_state(
    State(state): State<Arc<AppState>>,
    Path((vault_name, entity_id)): Path<(String, Uuid)>,
    Json(body): Json<PlaybackStateInput>,
) -> ApiResult<Json<archiveos_contract::PlaybackStateResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    Ok(Json(vault.upsert_playback_state(entity_id, &body)?))
}

async fn dismiss_playback_state(
    State(state): State<Arc<AppState>>,
    Path((vault_name, entity_id)): Path<(String, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    vault.dismiss_playback_state(entity_id)?;
    Ok(Json(serde_json::json!({ "entity_id": entity_id, "status": "dismissed" })))
}

async fn create_user_list(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
    Json(body): Json<CreateUserListInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    let list_id = vault.create_user_list(&body)?;
    Ok(Json(serde_json::json!({ "id": list_id })))
}

async fn add_user_list_member(
    State(state): State<Arc<AppState>>,
    Path((vault_name, list_id)): Path<(String, Uuid)>,
    Json(body): Json<AddUserListMemberInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    vault.add_user_list_member(list_id, &body)?;
    Ok(Json(serde_json::json!({
        "list_id": list_id,
        "entity_id": body.entity_id,
        "status": "added",
    })))
}

async fn remove_user_list_member(
    State(state): State<Arc<AppState>>,
    Path((vault_name, list_id, entity_id)): Path<(String, Uuid, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    vault.remove_user_list_member(list_id, entity_id)?;
    Ok(Json(serde_json::json!({
        "list_id": list_id,
        "entity_id": entity_id,
        "status": "removed",
    })))
}

async fn reorder_user_list_members(
    State(state): State<Arc<AppState>>,
    Path((vault_name, list_id)): Path<(String, Uuid)>,
    Json(body): Json<ReorderUserListMembersInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    vault.reorder_user_list_members(list_id, &body.entity_ids)?;
    Ok(Json(serde_json::json!({
        "list_id": list_id,
        "status": "reordered",
    })))
}

async fn regenerate_previews(
    State(state): State<Arc<AppState>>,
    Path((vault_name, entity_id)): Path<(String, Uuid)>,
) -> ApiResult<Json<JobResponse>> {
    let vault = open_vault(&state, &vault_name)?;
    let _ = vault.get_entity(entity_id)?;
    let job = vault.regenerate_preview_job(&vault_name, entity_id)?;
    Ok(Json(JobResponse::from(job)))
}

async fn backfill_previews(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
) -> ApiResult<Json<archiveos_core::preview::PreviewBackfillReport>> {
    let vault = open_vault(&state, &vault_name)?;
    Ok(Json(vault.backfill_preview_jobs(&vault_name)?))
}

#[derive(Deserialize)]
struct CommitPreviewJobRequest {
    entity_id: Uuid,
    files: Vec<archiveos_core::preview::PreviewFileCommit>,
}

async fn commit_preview_job(
    State(state): State<Arc<AppState>>,
    Path((vault_name, job_id)): Path<(String, Uuid)>,
    Json(body): Json<CommitPreviewJobRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    if vault.get_job(job_id)?.is_none() {
        return Err(archiveos_contract::VaultError::NotFound.into());
    }
    vault.commit_preview_files(job_id, body.entity_id, &body.files)?;
    vault.finish_job(job_id, "done")?;
    Ok(Json(serde_json::json!({
        "job_id": job_id,
        "entity_id": body.entity_id,
        "files": body.files.len(),
        "status": "done",
    })))
}

fn maybe_bind_subscription_collection(
    vault: &Vault,
    job_id: Uuid,
    manifest: &ImportManifest,
) -> Result<(), archiveos_contract::VaultError> {
    let Some(collection) = &manifest.collection else {
        return Ok(());
    };
    let Some(job) = vault.get_job(job_id)? else {
        return Ok(());
    };
    let input = YtdlpJobInput::normalize(&job.input)?;
    if input.mode != "subscription" {
        return Ok(());
    }
    let entity_id = archiveos_core::import::db::find_entity_id_by_source_url(
        vault.connection(),
        &collection.url,
    )?
    .or_else(|| {
        archiveos_core::import::db::find_entity_by_source_ref(
            vault.connection(),
            manifest.source_identity.as_deref().unwrap_or("youtube"),
            "playlist",
            &collection.external_id,
        )
        .ok()
        .flatten()
    });
    let Some(entity_id) = entity_id else {
        return Ok(());
    };
    archiveos_core::subscriptions::bind_subscription_collection(
        vault.connection(),
        &input.url,
        entity_id,
    )
}

fn enqueue_preview_jobs_from_manifest(
    vault: &Vault,
    vault_name: &str,
    manifest: &ImportManifest,
    parent_job_id: Uuid,
) -> Result<(), archiveos_contract::VaultError> {
    for item in &manifest.items {
        if item.status != "complete" {
            continue;
        }
        let Some(source_ref) = &item.source_ref else {
            continue;
        };
        let Some(entity_id) = archiveos_core::import::db::find_entity_by_source_ref(
            vault.connection(),
            &source_ref.source,
            &source_ref.kind,
            &source_ref.external_id,
        )?
        else {
            continue;
        };
        let _ = vault.maybe_enqueue_preview_job(vault_name, entity_id, Some(parent_job_id))?;
    }
    Ok(())
}
