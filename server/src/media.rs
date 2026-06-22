use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;

use archiveos_contract::VaultError;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::error::ApiResult;
use crate::routes::open_vault;
use crate::AppState;

pub async fn asset_content(
    State(state): State<Arc<AppState>>,
    Path((vault_name, asset_id)): Path<(String, Uuid)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let vault = open_vault(&state, &vault_name)?;
    let (path, mime) = vault.resolve_asset_content_path(asset_id)?;
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());
    stream_file(path, mime, range).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end: u64,
}

enum RangeParseError {
    Invalid,
    Unsatisfiable,
}

fn parse_range_header(value: &str, file_size: u64) -> Result<Option<ByteRange>, RangeParseError> {
    if file_size == 0 {
        return Err(RangeParseError::Unsatisfiable);
    }

    let Some(spec) = value.strip_prefix("bytes=") else {
        return Ok(None);
    };

    if spec.contains(',') {
        return Ok(None);
    }

    let Some((start_str, end_str)) = spec.split_once('-') else {
        return Err(RangeParseError::Invalid);
    };

    let range = if start_str.is_empty() {
        let suffix: u64 = end_str
            .parse()
            .map_err(|_| RangeParseError::Invalid)?;
        if suffix == 0 || suffix > file_size {
            return Err(RangeParseError::Unsatisfiable);
        }
        let start = file_size - suffix;
        ByteRange {
            start,
            end: file_size - 1,
        }
    } else {
        let start: u64 = start_str
            .parse()
            .map_err(|_| RangeParseError::Invalid)?;
        let end = if end_str.is_empty() {
            file_size - 1
        } else {
            end_str
                .parse()
                .map_err(|_| RangeParseError::Invalid)?
        };
        if start > end || start >= file_size {
            return Err(RangeParseError::Unsatisfiable);
        }
        ByteRange {
            start,
            end: end.min(file_size - 1),
        }
    };

    Ok(Some(range))
}

async fn stream_file(
    path: PathBuf,
    mime: String,
    range_header: Option<&str>,
) -> ApiResult<Response> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(VaultError::Io)?;
    let file_size = metadata.len();

    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(VaultError::Io)?;

    let (status, start, content_length, content_range) = match range_header {
        None => (StatusCode::OK, 0, file_size, None),
        Some(header_value) => match parse_range_header(header_value, file_size) {
            Ok(Some(range)) => {
                let length = range.end - range.start + 1;
                (
                    StatusCode::PARTIAL_CONTENT,
                    range.start,
                    length,
                    Some(format!(
                        "bytes {}-{}/{}",
                        range.start,
                        range.end,
                        file_size
                    )),
                )
            }
            Ok(None) => (StatusCode::OK, 0, file_size, None),
            Err(RangeParseError::Unsatisfiable) => {
                return Ok(Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{file_size}"))
                    .header(header::ACCEPT_RANGES, "bytes")
                    .body(Body::empty())
                    .map_err(|err| VaultError::InvalidLayout {
                        detail: err.to_string(),
                    })?);
            }
            Err(RangeParseError::Invalid) => (StatusCode::OK, 0, file_size, None),
        },
    };

    if start > 0 {
        file.seek(SeekFrom::Start(start))
            .await
            .map_err(VaultError::Io)?;
    }

    let stream = ReaderStream::new(file.take(content_length));
    let body = Body::from_stream(stream);

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, content_length.to_string());

    if let Some(content_range) = content_range {
        builder = builder.header(header::CONTENT_RANGE, content_range);
    }

    Ok(builder
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

    fn insert_thumbnail_asset(vault: &Vault) -> (Uuid, Uuid) {
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

        (entity_id, asset_id)
    }

    #[tokio::test]
    async fn asset_content_streams_present_managed_asset() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let (_entity_id, asset_id) = insert_thumbnail_asset(&vault);

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
        assert_eq!(
            response.headers().get(header::CONTENT_LENGTH).unwrap(),
            "11"
        );
        assert_eq!(
            response.headers().get(header::ACCEPT_RANGES).unwrap(),
            "bytes"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"thumb-bytes");
    }

    #[tokio::test]
    async fn asset_content_serves_bounded_range() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let (_entity_id, asset_id) = insert_thumbnail_asset(&vault);

        let (app, _registry_dir) = test_app(&vault, "vault");
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/vaults/vault/assets/{asset_id}/content"))
                    .header(header::RANGE, "bytes=0-4")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 0-4/11"
        );
        assert_eq!(
            response.headers().get(header::CONTENT_LENGTH).unwrap(),
            "5"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"thumb");
    }

    #[tokio::test]
    async fn asset_content_serves_open_ended_range() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let (_entity_id, asset_id) = insert_thumbnail_asset(&vault);

        let (app, _registry_dir) = test_app(&vault, "vault");
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/vaults/vault/assets/{asset_id}/content"))
                    .header(header::RANGE, "bytes=6-")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 6-10/11"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"bytes");
    }

    #[tokio::test]
    async fn asset_content_serves_suffix_range() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let (_entity_id, asset_id) = insert_thumbnail_asset(&vault);

        let (app, _registry_dir) = test_app(&vault, "vault");
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/vaults/vault/assets/{asset_id}/content"))
                    .header(header::RANGE, "bytes=-5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            response.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes 6-10/11"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"bytes");
    }

    #[tokio::test]
    async fn asset_content_rejects_unsatisfiable_range() {
        let dir = tempdir().unwrap();
        let vault = Vault::init(dir.path()).unwrap();
        let (_entity_id, asset_id) = insert_thumbnail_asset(&vault);

        let (app, _registry_dir) = test_app(&vault, "vault");
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/vaults/vault/assets/{asset_id}/content"))
                    .header(header::RANGE, "bytes=100-200")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(
            response.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes */11"
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
