use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultRegistryEntry {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub status: VaultStatus,
    pub registered_at: DateTime<Utc>,
}
