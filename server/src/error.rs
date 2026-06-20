use archiveos_contract::VaultError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub struct ApiError(VaultError);

impl From<VaultError> for ApiError {
    fn from(value: VaultError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            VaultError::NotFound | VaultError::RegistryNotFound { .. } => {
                (StatusCode::NOT_FOUND, self.0.to_string())
            }
            VaultError::AlreadyExists | VaultError::RegistryConflict { .. } => {
                (StatusCode::CONFLICT, self.0.to_string())
            }
            VaultError::VaultUnavailable { .. } => (StatusCode::SERVICE_UNAVAILABLE, self.0.to_string()),
            VaultError::InvalidLayout { .. } => (StatusCode::BAD_REQUEST, self.0.to_string()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
