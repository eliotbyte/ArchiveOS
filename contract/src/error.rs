#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("vault already exists at this path")]
    AlreadyExists,

    #[error("vault not found")]
    NotFound,

    #[error("invalid vault layout: {detail}")]
    InvalidLayout { detail: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("database error: {detail}")]
    Database { detail: String },

    #[error("registry entry not found: {name}")]
    RegistryNotFound { name: String },

    #[error("vault '{name}' is unavailable")]
    VaultUnavailable { name: String },

    #[error("registry conflict: {detail}")]
    RegistryConflict { detail: String },
}
