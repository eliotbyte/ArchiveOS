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
        .route("/vaults/{vault}/jobs/{id}/manifest", post(submit_manifest))
        .route("/vaults/{vault}/sources/has", get(sources_has))
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
}

async fn register_vault(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterVaultRequest>,
) -> ApiResult<Json<VaultRegistryEntry>> {
    let registry = state.registry()?;
    Ok(Json(registry.register(&body.name, &body.path)?))
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
        }
    }
}

#[derive(Deserialize)]
struct SearchParams {
    tag: Option<String>,
    query: Option<String>,
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
    let lease_secs = body.map(|b| b.lease_secs).unwrap_or_else(default_lease);
    let job = vault.claim_job(lease_secs)?;
    Ok(Json(job.map(JobResponse::from)))
}

async fn submit_manifest(
    State(state): State<Arc<AppState>>,
    Path((vault_name, job_id)): Path<(String, Uuid)>,
    Json(manifest): Json<ImportManifest>,
) -> ApiResult<Json<serde_json::Value>> {
    let vault = open_vault(&state, &vault_name)?;
    let job = vault.get_job(job_id)?;
    if job.is_none() {
        return Err(archiveos_contract::VaultError::NotFound.into());
    }

    let staging = vault.staging_job_dir(&job_id.to_string());
    let report = vault.import(&staging, Some(&manifest), ImportStrategy::Managed)?;
    vault.finish_job(job_id, "done")?;

    Ok(Json(serde_json::json!({
        "job_id": job_id,
        "entities_created": report.entities_created,
        "entities_reused": report.entities_reused,
        "blobs_stored": report.blobs_stored,
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
