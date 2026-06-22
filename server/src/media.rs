use std::path::PathBuf;
use std::sync::Arc;

use archiveos_contract::VaultError;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::routes::open_vault;
use crate::AppState;

pub async fn asset_content(
    State(state): State<Arc<AppState>>,
    Path((vault_name, asset_id)): Path<(String, Uuid)>,
) -> ApiResult<Response> {
    let vault = open_vault(&state, &vault_name)?;
    let (path, mime) = vault.resolve_asset_content_path(asset_id)?;
    stream_file(path, mime).await
}

async fn stream_file(path: PathBuf, mime: String) -> ApiResult<Response> {
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(VaultError::Io)?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(body)
        .map_err(|err| VaultError::InvalidLayout {
            detail: err.to_string(),
        })?)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use archiveos_core::{Registry, Vault};
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use axum::Router;
    use tempfile::tempdir;
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::routes;
    use crate::AppState;

    fn test_app(vault: &Vault, name: &str) -> (Router, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let registry = Registry::open(dir.path()).unwrap();
        registry.register(name, vault.root()).unwrap();
        let state = Arc::new(AppState {
            config_dir: dir.path().to_path_buf(),
        });
        let app = Router::new()
            .merge(routes::router())
            .with_state(state);
        (app, dir)
    }

    #[tokio::test]
    async fn asset_content_streams_present_managed_asset() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let entity_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();

        let staging = vault.staging_job_dir("test-job").join("files/thumb.jpg");
        std::fs::create_dir_all(staging.parent().unwrap()).unwrap();
        std::fs::write(&staging, b"thumb-bytes").unwrap();
        let cas = vault.store_managed(&staging, "jpg").unwrap();

        vault.connection()
            .execute(
                "INSERT INTO entity (id, content_hash, mime, size, status, added_at)
                 VALUES (?1, NULL, 'video/mp4', 0, 'present', ?2)",
                rusqlite::params![entity_id.to_string(), now],
            )
            .unwrap();
        vault.connection()
            .execute(
                "INSERT INTO entity_asset (
                    id, entity_id, role, kind, content_hash, mime, size, ext, status,
                    storage_strategy, path, created_at, updated_at, deleted_at
                 ) VALUES (?1, ?2, 'supporting', 'thumbnail', ?3, 'image/jpeg', ?4, '.jpg',
                           'present', 'managed', ?5, ?6, ?6, NULL)",
                rusqlite::params![
                    asset_id.to_string(),
                    entity_id.to_string(),
                    cas.content_hash,
                    cas.size as i64,
                    cas.blob_path.to_string_lossy().to_string(),
                    now,
                ],
            )
            .unwrap();

        let (app, _registry_dir) = test_app(&vault, "vault");
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/vaults/vault/assets/{asset_id}/content"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/jpeg"
        );
    }

    #[tokio::test]
    async fn asset_content_rejects_remote_asset() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let entity_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();

        vault.connection()
            .execute(
                "INSERT INTO entity (id, content_hash, mime, size, status, added_at)
                 VALUES (?1, NULL, 'video/mp4', 0, 'present', ?2)",
                rusqlite::params![entity_id.to_string(), now],
            )
            .unwrap();
        vault.connection()
            .execute(
                "INSERT INTO entity_asset (
                    id, entity_id, role, kind, content_hash, mime, size, ext, status,
                    storage_strategy, path, created_at, updated_at, deleted_at
                 ) VALUES (?1, ?2, 'supporting', 'subtitle', NULL, NULL, 0, NULL,
                           'remote', 'remote', NULL, ?3, ?3, NULL)",
                rusqlite::params![asset_id.to_string(), entity_id.to_string(), now],
            )
            .unwrap();

        let (app, _registry_dir) = test_app(&vault, "vault");
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/vaults/vault/assets/{asset_id}/content"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn list_entities_returns_recent_items() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let entity_id = Uuid::new_v4();
        let now = chrono::Utc::now().to_rfc3339();
        vault.connection()
            .execute(
                "INSERT INTO entity (id, content_hash, mime, size, status, added_at)
                 VALUES (?1, NULL, 'video/mp4', 0, 'present', ?2)",
                rusqlite::params![entity_id.to_string(), now],
            )
            .unwrap();
        vault.connection()
            .execute(
                "INSERT INTO metadata (entity_id, key, value, provenance)
                 VALUES (?1, 'title', 'Sample Video', 'user')",
                [entity_id.to_string()],
            )
            .unwrap();

        let (app, _registry_dir) = test_app(&vault, "vault");
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/vaults/vault/entities?limit=10")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let items: Vec<archiveos_contract::EntityListItem> = serde_json::from_slice(&body).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, entity_id);
        assert_eq!(items[0].title.as_deref(), Some("Sample Video"));
    }
}
