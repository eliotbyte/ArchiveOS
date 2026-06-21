use std::sync::Arc;

use archiveos_contract::{ImportManifest, ImportStrategy, SearchQuery, VaultRegistryEntry};
use archiveos_core::{open_vault_ref, Vault};
use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
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
        .route("/vaults/{vault}/entities/{id}", get(get_entity))
        .route("/vaults/{vault}/search", get(search_entities))
        .route("/vaults/{vault}/entities/{id}/tags", post(add_tag))
        .route(
            "/vaults/{vault}/entities/{id}/tags/{tag}",
            delete(remove_tag),
        )
        .route("/vaults/{vault}/jobs", post(create_job))
        .route("/vaults/{vault}/jobs/claim", post(claim_job))
        .route("/vaults/{vault}/jobs/{id}/heartbeat", post(heartbeat_job))
        .route("/vaults/{vault}/jobs/{id}/manifest", post(submit_manifest))
        .route("/vaults/{vault}/sources/has", get(sources_has))
        .route("/vaults/{vault}/source-failures", get(list_source_failures).post(record_source_failure))
        .route("/vaults/{vault}/subscriptions", get(list_subscriptions).post(create_subscription))
        .route("/vaults/{vault}/subscriptions/{id}", delete(delete_subscription))
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

fn open_vault(state: &AppState, vault: &str) -> Result<Vault, ApiError> {
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

#[derive(Serialize)]
struct EntityDetailResponse {
    id: Uuid,
    content_hash: Option<String>,
    mime: Option<String>,
    size: i64,
    status: String,
    added_at: String,
    created_at: Option<String>,
    tags: Vec<String>,
    metadata: std::collections::HashMap<String, String>,
    metadata_entries: Vec<archiveos_contract::MetadataEntry>,
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
        }
    }
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
}

async fn heartbeat_job(
    State(state): State<Arc<AppState>>,
    Path((vault_name, job_id)): Path<(String, Uuid)>,
    body: Option<Json<HeartbeatJobRequest>>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    let lease_secs = body.map(|b| b.lease_secs).unwrap_or_else(default_lease);
    vault.heartbeat_job(job_id, lease_secs)?;
    Ok(Json(serde_json::json!({ "job_id": job_id, "ok": true })))
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
            })
            .collect(),
    ))
}

#[derive(Serialize)]
struct SourceHasResponse {
    external_id: String,
    entity_id: String,
    present: bool,
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
    })?;
    Ok(Json(SubscriptionResponse::from(sub)))
}

async fn list_subscriptions(
    State(state): State<Arc<AppState>>,
    Path(vault_name): Path<String>,
) -> ApiResult<Json<Vec<SubscriptionResponse>>> {
    let vault = open_vault(&state, &vault_name)?;
    let subs = vault.list_subscriptions()?;
    Ok(Json(subs.into_iter().map(SubscriptionResponse::from).collect()))
}

async fn delete_subscription(
    State(state): State<Arc<AppState>>,
    Path((vault_name, id)): Path<(String, Uuid)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    vault.delete_subscription(id)?;
    Ok(Json(serde_json::json!({ "removed": id })))
}
